throw 'SOURCE_ONLY_DISABLED_STAGE19_ADAPTER003_PENDING_NO_VM_OR_CREDENTIAL_ENTRY'
# PRIVATE fixed host controller template. NO execution until successor is bound/reviewed.
Set-StrictMode -Version Latest
$ErrorActionPreference='Stop'
if ('C28_SOURCE_BOUND_AND_REVIEWED' -cne 'C28_SOURCE_BOUND_AND_REVIEWED') { throw 'Blocked candidate; no credential or VM action.' }
$session=$null;$started=$false;$installationReserved=$false;$guestPid=$null;$actualExit=$null;$rawEOF=$false;$terminalJournalLength=$null;$journalCaughtUp=$false
$pollAttempts=0;$captureUnknown=$false;$transportUnknown=$false;$pollInFlight=$false;$pollCompleted=$true;$terminalRecordAttempted=$false;$captureTerminal=@{Pid=$null;Exit=$null;Unknown=$false};$hostCapture=@{};$lastGuestCapture=$null
$hostUnknown=$false;$record=0;$attempt=0;$used=@{};$pending='';$pins=@();$rawHandles=@{}
$taskSecurePassword=$null;$taskCredential=$null
$group=$null;$usedGroups=@{};$observedBlocked=$false
$fixedGroups=@{
    'session-four'=@('sealed-session-context','sealed-session-worker','sealed-session-run','sealed-session-join')
    'native-eight'=@('sealed-native-context','sealed-native-worker','sealed-propose','sealed-trusted-review','sealed-trusted-approve-claim','sealed-native-start','sealed-basic-observe','sealed-native-join')
}
$groupConfirmations=@{
    'session-four'='RUN_ORIGINAL_SESSION_FOUR_ONCE_WITH_NO_HUMAN_PAUSE'
    'native-eight'='RUN_ORIGINAL_NATIVE_EIGHT_ONCE_WITH_NO_HUMAN_PAUSE'
}
function Write-ControlNew([string]$Path,$Value) {
    $raw=[Text.UTF8Encoding]::new($false).GetBytes(($Value | ConvertTo-Json -Depth 6 -Compress)+"`n")
    if($raw.Length -gt 4096){throw 'Small fixed control record bound.'}
    $f=[IO.File]::Open($Path,[IO.FileMode]::CreateNew,[IO.FileAccess]::Write,[IO.FileShare]::Read)
    try{$f.Write($raw,0,$raw.Length);$f.Flush($true)}finally{$f.Dispose()}
}
function Assert-CapturedOriginalJournal([long]$ExpectedLength) {
    $rawHandles['journal'].Flush($true)
    $f=[IO.File]::Open((Join-Path $root 'journal.log'),[IO.FileMode]::Open,[IO.FileAccess]::Read,[IO.FileShare]::ReadWrite)
    try {
        if ($f.Length -ne $ExpectedLength -or $f.Length -gt 33554432) { throw 'Final local journal size mismatch.' }
        $b=New-Object byte[] ([int]$f.Length);$n=0
        while($n -lt $b.Length){$r=$f.Read($b,$n,$b.Length-$n);if($r -eq 0){throw 'Final journal shortened.'};$n+=$r}
    } finally {$f.Dispose()}
    if($b.Length -eq 0 -or $b[$b.Length-1] -ne 10){throw 'Final original journal not complete newline-delimited records.'}
    $text=[Text.UTF8Encoding]::new($false,$true).GetString($b);$seq=0
    foreach($line in $text.Split([char]10)) {
        if($line.Length -eq 0){continue}
        if([Text.Encoding]::UTF8.GetByteCount($line) -gt 131072 -or $seq -ge 256){throw 'Final journal record bound.'}
        $r=$line | ConvertFrom-Json
        if([int]$r.sequence -ne $seq){throw 'Final original journal sequence mismatch.'};$seq++
    }
    # Parsing copied evidence does not create/reconstruct any runtime authority.
}
function Receive-C28CaptureFrame($Frame) {
    # Never unwind ForEach-Object on host receive/save faults. Consume the whole RPC.
    try {
    if ([int]$Frame.Pid -ne $script:guestPid) { throw 'Different process; no adoption.' }
    if ($Frame.Kind -ceq 'TERMINAL') {
        Observe-C28Terminal $script:captureTerminal ([int]$Frame.Pid) ([int]$Frame.ActualExit)
        $script:actualExit=$script:captureTerminal.Exit
        if (-not $script:terminalRecordAttempted) {
            $script:terminalRecordAttempted=$true
            try { Write-ControlNew (Join-Path $script:root 'guest-actual-terminal.json') ([ordered]@{GuestPid=$script:guestPid;ActualExit=$script:actualExit;State='TERMINAL_VALIDATION_PENDING';NoSDKAcceptance=$true}) }
            catch { $script:hostUnknown=$true;$script:captureUnknown=$true }
        }
    } elseif ($Frame.Kind -ceq 'CAPTURE_BYTES') {
        $n=[string]$Frame.Stream
        if (-not $script:hostCapture.ContainsKey($n) -or ([string]$Frame.Bytes).Length -gt 21848) { throw 'Fixed capture frame bound.' }
        $b=[Convert]::FromBase64String([string]$Frame.Bytes);$limit=if($n -ceq 'journal'){33554432}else{131072}
        Write-C28HostFrame $script:hostCapture[$n] $script:rawHandles[$n] ([long]$Frame.Offset) $b $limit
        $script:rawTotals[$n]=$script:hostCapture[$n].Saved
        if ($script:hostCapture[$n].Unknown) { $script:hostUnknown=$true;$script:captureUnknown=$true }
    } elseif ($Frame.Kind -ceq 'OBSERVATION') {
        $script:pending=[string]$Frame.PendingCommand;$script:rawEOF=[bool]$Frame.ReadersActuallyEOF;$script:observedBlocked=[bool]$Frame.Blocked
        $script:lastGuestCapture=$Frame.Capture
        foreach($n in @('Out','Err','journal')) {
            $g=$Frame.Capture[$n];$h=$script:hostCapture[$n]
            if ($g.SavedAck -gt $h.Saved -or $g.Observed -lt $h.Received) { throw 'Guest capture counters mismatch.' }
            if ($g.DrainOnly -or $g.Unknown) { $h.DrainOnly=$true;$h.Unknown=$true;$script:hostUnknown=$true }
            if ($g.EOF) { $h.EOF=$true }
        }
        if ($Frame.CaptureFailed) { $script:hostUnknown=$true;$script:captureUnknown=$true }
        if ($null -ne $Frame.TerminalJournalLength) {
            if ($null -eq $script:actualExit -or -not $Frame.Terminal -or $Frame.TerminalJournalLength -lt 0 -or $Frame.TerminalJournalLength -gt 33554432) { throw 'Uncorrelated terminal journal length.' }
            if ($null -ne $script:terminalJournalLength -and $script:terminalJournalLength -ne $Frame.TerminalJournalLength) { throw 'Frozen terminal journal changed.' }
            $script:terminalJournalLength=[long]$Frame.TerminalJournalLength
        }
        $script:journalCaughtUp=[bool]$Frame.JournalCaughtUp
        if ($script:record -lt 4096) {
            $script:record++
            try { Write-ControlNew (Join-Path $script:root ('poll-{0:D4}.json' -f $script:record)) ([ordered]@{GuestPid=$script:guestPid;PendingCommand=$script:pending;Terminal=$Frame.Terminal;ReadersEOF=$script:rawEOF;Capture=$Frame.Capture;Unknown=$script:hostUnknown;Scope='CAPTURE_NOT_PRODUCT_JOIN_OR_ACCEPTANCE'}) }
            catch { $script:hostUnknown=$true;$script:captureUnknown=$true }
        } else { $script:hostUnknown=$true }
    } else { throw 'Unexpected remote capture frame.' }
    } catch {
        # A malformed/unusable frame never authorizes another identity or a replay.
        # Keep consuming this invocation so transport completion remains observable.
        $script:hostUnknown=$true;$script:captureUnknown=$true
        foreach($n in @('Out','Err','journal')) { $script:hostCapture[$n].Unknown=$true;$script:hostCapture[$n].DrainOnly=$true }
    }
}
function Invoke-C28CapturePoll {
    if ($script:pollAttempts -ge 4096) {
        $script:hostUnknown=$true;$script:captureUnknown=$true
        # Hard scope ceiling: retain Unknown and do not send another observation.
        return
    }
    if ($script:transportUnknown -or $script:pollInFlight -or -not $script:pollCompleted) { throw 'Ambiguous transport: no new observation RPC.' }
    $mask=0;$i=0
    foreach($n in @('Out','Err','journal')) { if($script:hostCapture[$n].DrainOnly){$mask=$mask -bor (1 -shl $i)};$i++ }
    $acks=@([long]$script:hostCapture.Out.Saved,[long]$script:hostCapture.Err.Saved,[long]$script:hostCapture.journal.Saved,[int]$mask)
    # Only observation RPC is repeatable; business commands are never retransmitted.
    $script:pollAttempts++;$script:pollInFlight=$true;$script:pollCompleted=$false
    try {
        Invoke-Command -Session $script:session -ArgumentList $acks -ScriptBlock {param($o,$e,$j,$d) Poll-C28One $o $e $j $d} -ErrorAction Stop | ForEach-Object { Receive-C28CaptureFrame $_ }
        $script:pollCompleted=$true
    } catch {
        # State=Opened is not evidence that an ambiguous remote invocation ended.
        $script:transportUnknown=$true;$script:hostUnknown=$true
        # pollCompleted deliberately stays false. No reconnect or new poll follows.
    } finally { $script:pollInFlight=$false }

}
# Only capture/save faults with a completed prior observation may observe in Park.
# Guest capture now drains autonomously; this loop is never required to read pipes.
function Park-Unknown {
    # Keep this process/PSSession/streams on the same thread. Never reopen or restore.
    try { Write-ControlNew (Join-Path $root 'unknown-retained.json') ([ordered]@{Status='UNKNOWN_SAME_CONTROLLER_RETAINED';GuestPid=$guestPid;ActualExitIfObserved=$actualExit;Reconnect=$false;Replay=$false;Kill=$false;Cleanup=$false}) } catch {}
    [Console]::Error.WriteLine('Unknown: same controller and original guest state retained; no replay, reconnect, kill or automatic cleanup.')
    while ($true) {
        if ($script:pollAttempts -lt 4096 -and $script:captureUnknown -and -not $script:transportUnknown -and $script:pollCompleted -and -not $script:pollInFlight -and $null -ne $script:session -and $null -ne $script:guestPid) {
            # Same original session, no business command, no reopening or adoption.
            # Terminal+EOF only end observation; they do not finish a product owner.
            if ($null -eq $script:actualExit -or -not $script:rawEOF -or $null -eq $script:lastGuestCapture -or -not $script:lastGuestCapture.journal.EOF) {
                Invoke-C28CapturePoll
                [Threading.Thread]::Sleep(10)
                continue
            }
        }
        [Threading.Thread]::Sleep(60000)
    }
}
function Read-ControllerLine {
    $s=[Text.StringBuilder]::new()
    while ($true) {
        $c=[Console]::In.Read()
        if ($c -eq -1) { return $null }
        if ($c -eq 10) { break }
        if ($s.Length -ge 512 -or $c -eq 0) { throw 'Fixed controller input bound.' }
        [void]$s.Append([char]$c)
    }
    $v=$s.ToString().TrimEnd([char]13)
    if ([Text.Encoding]::UTF8.GetByteCount($v) -gt 512) { throw 'Controller UTF8 line bound.' }
    return $v
}
try {
    if ($PSVersionTable.PSVersion.Major -ne 5 -or $PSVersionTable.PSVersion.Minor -ne 1) { throw 'Fixed Windows PowerShell 5.1 required.' }
    $hostExe=[Diagnostics.Process]::GetCurrentProcess().MainModule.FileName
    $h=[Security.Cryptography.SHA256]::Create()
    try { $actual=[BitConverter]::ToString($h.ComputeHash([IO.File]::ReadAllBytes($hostExe))).Replace('-','').ToLowerInvariant() } finally {$h.Dispose()}
    if ($hostExe -cne 'C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe' -or $actual -cne '8bb6fa8c283b4d92120b1ef249a9b311b0f804d4cabbe9981159976c8be76a5e') { throw 'Fixed host interpreter identity mismatch.' }
    $taskModules = @(
        @('C:\Windows\System32\WindowsPowerShell\v1.0\Modules\Microsoft.PowerShell.Utility\Microsoft.PowerShell.Utility.psd1', 'c09df190addc67f7c6c38e7ea1dca719fd87807107f688c3f60ed8816e1c48a6'),
        @('C:\Windows\System32\WindowsPowerShell\v1.0\Modules\Microsoft.PowerShell.Management\Microsoft.PowerShell.Management.psd1', '91ea580e6bbc54148eadac5e8018f6e6edfd04bcd3cf07489a9028fdeea948a0'),
        @('C:\Windows\System32\WindowsPowerShell\v1.0\Modules\Microsoft.PowerShell.Security\Microsoft.PowerShell.Security.psd1', 'fa7150089e8a67a0aad27cd324d119b9778ccbead6242397780c5d5077246d30'),
        @('C:\Windows\System32\WindowsPowerShell\v1.0\Modules\Hyper-V\2.0.0.0\Hyper-V.psd1', '5f4568b7825fed4c960ff3d0fbcf2d847d67b8ac4d5ec2b069339f5b3b4f4978')
    )
    foreach ($taskModule in $taskModules) {
        $taskHasher = [Security.Cryptography.SHA256]::Create()
        try { $taskActual = [BitConverter]::ToString($taskHasher.ComputeHash([IO.File]::ReadAllBytes($taskModule[0]))).Replace('-', '').ToLowerInvariant() } finally { $taskHasher.Dispose() }
        if ($taskActual -ne $taskModule[1]) { throw 'Fixed built-in manifest hash mismatch.' }
        Import-Module -Name $taskModule[0] -ErrorAction Stop
    }

    foreach ($pair in @(@('common.ps1','a569f451fa54a8dd3fd9a28f3a3068bb4498d1ab10eebcd4b9c0059a427227e8'),@('remote-live-definitions.ps1','b6da22cb50db502ad8fd2ceb289d95e0f933f73c7a496459ad0ee4120e23bc56'),@('capture-ack-state.ps1','320d929cff684330f62cb4b3b5e7e6aea69f4ae3fa3db28ff5fd791eae9b246c'),@('pump-install-contract.ps1','1b2c47756768f9a28c54d5b451659ee65fc25dfdc2287c11ab2a33b32ffc2f36'))) {
        $f=[IO.File]::Open((Join-Path $PSScriptRoot $pair[0]),[IO.FileMode]::Open,[IO.FileAccess]::Read,[IO.FileShare]::Read);$pins+=,$f
        $h=[Security.Cryptography.SHA256]::Create()
        try { $s=[BitConverter]::ToString($h.ComputeHash($f)).Replace('-','').ToLowerInvariant() } finally {$h.Dispose()}
        if ($s -cne $pair[1]) { throw 'Frozen definitions differ; no credential input.' }
    }
    . (Join-Path $PSScriptRoot 'capture-ack-state.ps1')
    . $CaptureAckDefinitions
    . (Join-Path $PSScriptRoot 'common.ps1')
    . (Join-Path $PSScriptRoot 'remote-live-definitions.ps1')
    . (Join-Path $PSScriptRoot 'pump-install-contract.ps1')
    Assert-C28Stage19Binding # PENDING fails before credential/session/Start.
    $manifest=Read-FixedManifest
    $root=Join-Path $PSScriptRoot 'stage19-records001';Assert-PlainPath $root
    if (Test-Path -LiteralPath $root) { throw 'Fresh host record directory required; no adoption.' }
    [void][IO.Directory]::CreateDirectory($root)
    foreach ($n in @('Out','Err','journal')) { $rawHandles[$n]=[IO.File]::Open((Join-Path $root ($n+'.log')),[IO.FileMode]::CreateNew,[IO.FileAccess]::Write,[IO.FileShare]::Read) }
    foreach($n in @('Out','Err','journal')) { $hostCapture[$n]=New-C28HostStream $n }
    $rawTotals=@{Out=0L;Err=0L;journal=0L};$commandMap=@{
    'begin'='READ_ACTUAL_LOCAL_HYPERV_GUEST_IDENTITY_AND_CREATE_FRESH_EVIDENCE'
    'inventory'='READ_ONLY_FIXED_SANDBOX_IDENTITY_AND_REGISTRATION_INVENTORY'
    'roots'='CREATE_ONLY_FRESH_SYNTHETIC_VM_ROOT'
    'setup'='APPLY_FIXED_SANDBOX_ACCOUNTS_DPAPI_ACL_WFP_IN_DISPOSABLE_VM'
    'materialize'='MATERIALIZE_AND_PIN_CHECKED_RUNNER_IN_OWN_SYNTHETIC_HOME'
    'open-owner'='OPEN_NEW_ORIGINAL_PROTECTED_WORKBENCH_IN_SYNTHETIC_ROOT'
    'production-factory'='CREATE_REAL_CHECKED_PRODUCTION_FACTORY_WITHOUT_STARTING_CHILD'
    'release-factory'='RELEASE_IDLE_FACTORY_AND_SYNCHRONOUSLY_JOIN_ITS_UNIQUE_SCHEDULER'
    'reap-factory-scheduler'='REAP_ONLY_THE_SAME_RETAINED_FACTORY_SCHEDULER'
    'finish-owner'='FINISH_ORIGINAL_OWNER_ONLY_AFTER_REAL_JOINS'
    'prepare-cleanup'='DISABLE_OWNED_SANDBOX_ACCOUNTS_AND_STOP_THEIR_PROCESSES'
    'finish-cleanup'='REMOVE_ONLY_OWNED_SANDBOX_RESOURCES_AND_PROTECTIONS'
    'sealed-basic'='CREATE_FRESH_OWNED_BASIC_FILES_AND_LOOPBACK_LISTENER_WITH_PINNED_GUEST_SID_QUERY'
    'sealed-native-start'='START_ONE_REVIEWED_CHECKED_SANDBOX_CHILD_IN_APPROVED_DISPOSABLE_GUEST'
    'sealed-trusted-approve-claim'='TRUSTED_APPROVE_AND_CLAIM_ONLY_THE_ACTUAL_SEALED_GUEST_PROPOSAL'
    'exit'='EXIT_ONLY_AFTER_ORIGINAL_FINISHED_LIFECYCLE_AND_REAL_JOINS'
    'sealed-session-review'='EXECUTE_ONE_ORIGINAL_SEALED_STEP_SESSION_REVIEW'
    'sealed-controls-review'='EXECUTE_ONE_ORIGINAL_SEALED_STEP_CONTROLS_REVIEW'
    'sealed-proposal-review'='EXECUTE_ONE_ORIGINAL_SEALED_STEP_PROPOSAL_REVIEW'
    'sealed-session-context'='EXECUTE_ONE_ORIGINAL_SEALED_STEP_SESSION_CONTEXT'
    'sealed-session-worker'='EXECUTE_ONE_ORIGINAL_SEALED_STEP_SESSION_WORKER'
    'sealed-session-run'='EXECUTE_ONE_ORIGINAL_SEALED_STEP_SESSION_RUN'
    'sealed-session-join'='EXECUTE_ONE_ORIGINAL_SEALED_STEP_SESSION_JOIN'
    'sealed-native-review'='EXECUTE_ONE_ORIGINAL_SEALED_STEP_NATIVE_REVIEW'
    'sealed-native-context'='EXECUTE_ONE_ORIGINAL_SEALED_STEP_NATIVE_CONTEXT'
    'sealed-native-worker'='EXECUTE_ONE_ORIGINAL_SEALED_STEP_NATIVE_WORKER'
    'sealed-propose'='EXECUTE_ONE_ORIGINAL_SEALED_STEP_PROPOSE'
    'sealed-trusted-review'='EXECUTE_ONE_ORIGINAL_SEALED_STEP_TRUSTED_REVIEW'
    'sealed-basic-observe'='EXECUTE_ONE_ORIGINAL_SEALED_STEP_BASIC_OBSERVE'
    'sealed-native-join'='EXECUTE_ONE_ORIGINAL_SEALED_STEP_NATIVE_JOIN'
    'sealed-cleanup'='EXECUTE_ONE_ORIGINAL_SEALED_STEP_CLEANUP'
    'sealed-repair-cleanup'='EXECUTE_ONE_ORIGINAL_SEALED_STEP_REPAIR_CLEANUP'
    'sealed-session-install'='EXECUTE_ONE_ORIGINAL_SEALED_STEP_SESSION_INSTALL'
    'sealed-session-base-select'='EXECUTE_ONE_ORIGINAL_SEALED_STEP_SESSION_BASE_SELECT'
    'sealed-session-base-enable'='EXECUTE_ONE_ORIGINAL_SEALED_STEP_SESSION_BASE_ENABLE'
    'sealed-session-wrapper-select'='EXECUTE_ONE_ORIGINAL_SEALED_STEP_SESSION_WRAPPER_SELECT'
    'sealed-session-approve'='EXECUTE_ONE_ORIGINAL_SEALED_STEP_SESSION_APPROVE'
    'sealed-session-wrapper-enable'='EXECUTE_ONE_ORIGINAL_SEALED_STEP_SESSION_WRAPPER_ENABLE'
    'sealed-controls-install'='EXECUTE_ONE_ORIGINAL_SEALED_STEP_CONTROLS_INSTALL'
    'sealed-controls-base-select'='EXECUTE_ONE_ORIGINAL_SEALED_STEP_CONTROLS_BASE_SELECT'
    'sealed-controls-base-enable'='EXECUTE_ONE_ORIGINAL_SEALED_STEP_CONTROLS_BASE_ENABLE'
    'sealed-controls-wrapper-select'='EXECUTE_ONE_ORIGINAL_SEALED_STEP_CONTROLS_WRAPPER_SELECT'
    'sealed-controls-approve'='EXECUTE_ONE_ORIGINAL_SEALED_STEP_CONTROLS_APPROVE'
    'sealed-controls-wrapper-enable'='EXECUTE_ONE_ORIGINAL_SEALED_STEP_CONTROLS_WRAPPER_ENABLE'
    'sealed-proposal-install'='EXECUTE_ONE_ORIGINAL_SEALED_STEP_PROPOSAL_INSTALL'
    'sealed-proposal-base-select'='EXECUTE_ONE_ORIGINAL_SEALED_STEP_PROPOSAL_BASE_SELECT'
    'sealed-proposal-base-enable'='EXECUTE_ONE_ORIGINAL_SEALED_STEP_PROPOSAL_BASE_ENABLE'
    'sealed-proposal-wrapper-select'='EXECUTE_ONE_ORIGINAL_SEALED_STEP_PROPOSAL_WRAPPER_SELECT'
    'sealed-proposal-approve'='EXECUTE_ONE_ORIGINAL_SEALED_STEP_PROPOSAL_APPROVE'
    'sealed-proposal-wrapper-enable'='EXECUTE_ONE_ORIGINAL_SEALED_STEP_PROPOSAL_WRAPPER_ENABLE'
    }
    Write-ControlNew (Join-Path $root 'controller-preflight.json') ([ordered]@{OperationId='7ba550b9-ad7f-499b-a56f-9711d9560856';HostPid=$PID;VmId='PENDING_EXPLICIT_HOST_VM_ID';State='WAITING_EXPLICIT_START';NoSystemAcceptance=$true;CredentialLogging='NONE'})
    $line=Read-ControllerLine
    if ($line -cne 'start|START_ONE_PINNED_C28_LIVE_HARNESS_IN_APPROVED_DISPOSABLE_GUEST') { throw 'Separate Stage3 authorization/start confirmation required.' }
    # Explicit manual input only. Never read a credential file or emit private data.
    $taskCredential=Read-C28ManualPrivateCredential $C28Stage19Binding.PrivateCallerName
    $taskSecurePassword=$taskCredential.Password

    $session=New-FixedConnection $taskCredential
    $taskCredential=$null;$taskSecurePassword.Dispose();$taskSecurePassword=$null
    $facts=@(Invoke-Command -Session $session -ScriptBlock $FixedGuestCheck -ErrorAction Stop)
    if ($facts.Count -ne 1 -or $facts[0].Schema -cne 'morrow-c28-fresh-guest-check-v1') { throw 'Fresh original guest precheck incomplete.' }
    Write-ControlNew (Join-Path $root 'fresh-guest-check.json') ([ordered]@{Administrator=[bool]$facts[0].Administrator;FirmwareUuid=[string]$facts[0].FirmwareUuid;AttemptAbsent=[bool]$facts[0].AttemptAbsent;FixedCollisionFree=[bool]$facts[0].FixedCollisionFree})
    Invoke-Command -Session $session -ScriptBlock $CaptureAckDefinitions -ErrorAction Stop | Out-Null
    Invoke-Command -Session $session -ScriptBlock $InstallC28PumpDefinitions -ErrorAction Stop | Out-Null
    $installationReserved=$true # Accepted/lost installation RPC is Unknown; retain the same PSSession.
    $installed=@(Invoke-Command -Session $session -ArgumentList $C28Stage19Binding.OperationId,'INSTALL_ONE_PINNED_CAPTURE_PUMP_FOR_7ba550b9-ad7f-499b-a56f-9711d9560856',$C28Stage19Binding.OnceAuthorizationSHA256 -ScriptBlock {param($o,$c,$a) Install-C28ReviewedPump $o $c $a} -ErrorAction Stop)
    if($installed.Count -ne 1 -or $installed[0].Kind -cne 'PUMP_INSTALLED' -or $installed[0].OperationId -cne $C28Stage19Binding.OperationId -or $installed[0].DllSHA256 -cne 'bf5ae8c16ee79acb8de4c00510589af532376674048381fbc369228e793273fc'){throw 'Explicit assembly installation record differs; no harness Start.'}
    Invoke-Command -Session $session -ScriptBlock $InstallLiveDefinitions -ErrorAction Stop | Out-Null
    Write-ControlNew (Join-Path $root 'start-reserved-unknown.json') ([ordered]@{OperationId='7ba550b9-ad7f-499b-a56f-9711d9560856';HostPid=$PID;Attempt=1;NeverRetry=$true})
    $started=$true # Any accepted start RPC loss is Unknown; even missing PID cannot be retried.
    $mb=[IO.File]::ReadAllBytes((Join-Path $PSScriptRoot 'public-manifest.json'))
    Invoke-Command -Session $session -ArgumentList (,$mb) -ScriptBlock {param([byte[]]$b) Start-C28OneProcess $b} -ErrorAction Stop | ForEach-Object {
        if ($_.Kind -cne 'STARTED' -or $_.OperationId -cne $C28Stage19Binding.OperationId -or $null -ne $guestPid) { throw 'Unexpected actual start record.' }
        $guestPid=[int]$_.Pid
        Write-ControlNew (Join-Path $root 'guest-started.json') ([ordered]@{OperationId='7ba550b9-ad7f-499b-a56f-9711d9560856';GuestPid=$guestPid;HostPid=$PID;Scope='ORIGINAL_LIVE_PROCESS_NOT_ACCEPTANCE'})
    }
    if ($null -eq $guestPid) { throw 'Start outcome Unknown; no retry.' }
    while ($true) {
        if ($null -ne $group) {
            if ($observedBlocked) {
                Write-ControlNew (Join-Path $root ('group-'+$group.Name+'-stopped.json')) ([ordered]@{Group=$group.Name;Status='ORIGINAL_RESULT_BLOCKED_QUEUE_STOPPED';SameControllerRetained=$true;NextIndex=$group.Index;Pending=$pending;NoReplay=$true;NoCleanup=$true})
                [Console]::Out.WriteLine('QUEUE_STOPPED_ORIGINAL_BLOCKED; explicit same-owner cleanup only.');[Console]::Out.Flush();$group=$null
                continue
            }
            if ($pending -ceq '' -and $group.Index -eq $group.Commands.Count) {
                Write-ControlNew (Join-Path $root ('group-'+$group.Name+'-returned.json')) ([ordered]@{Group=$group.Name;Status='ALL_ORIGINAL_STEP_RESULTS_OBSERVED_NOT_ACCEPTANCE';ElapsedMilliseconds=$group.Clock.ElapsedMilliseconds;SameControllerRetained=$true;NoTTLRefresh=$true})
                [Console]::Out.WriteLine('QUEUE_ORIGINAL_RESULTS_OBSERVED_NOT_SDK_ACCEPTANCE');[Console]::Out.Flush();$group=$null
                continue
            }
            if ($group.Clock.ElapsedMilliseconds -ge 40000 -or $record -ge 4096) {
                Write-ControlNew (Join-Path $root ('group-'+$group.Name+'-stopped.json')) ([ordered]@{Group=$group.Name;Status='OBSERVATION_BUDGET_STOPPED_OUTCOME_NOT_RECLASSIFIED';SameControllerRetained=$true;NextIndex=$group.Index;Pending=$pending;NoTTLRefresh=$true;NoReplay=$true;Kill=$false;Cleanup=$false})
                [Console]::Out.WriteLine('QUEUE_BUDGET_STOPPED_UNKNOWN_ORIGINAL_DEBT_RETAINED');[Console]::Out.Flush();$group=$null
                continue
            }
            if ($pending -cne '') { Start-Sleep -Milliseconds 10;$line='poll' }
            else {
                $c=$group.Commands[$group.Index];$group.Index++
                $line='dispatch|'+$c+'|'+$commandMap[$c]
            }
        } else { $line=Read-ControllerLine }
        if ($null -eq $line) { Park-Unknown }
        if ($hostUnknown -and $line -cne 'poll') { Park-Unknown }
        if ($line -ceq 'poll') {
            Invoke-C28CapturePoll
        } elseif ($line.StartsWith('group|')) {
            $parts=$line.Split('|')
            if ($parts.Count -ne 3 -or -not $fixedGroups.ContainsKey($parts[1]) -or $parts[2] -cne $groupConfirmations[$parts[1]] -or $usedGroups.ContainsKey($parts[1]) -or $null -ne $group -or $pending -cne '' -or $observedBlocked -or $null -ne $actualExit) { throw 'Only one exact pre-reviewed fixed original group, without pending or replay.' }
            $name=$parts[1];$list=$fixedGroups[$name]
            foreach($c in $list){if($used.ContainsKey($c) -or -not $commandMap.ContainsKey($c)){throw 'Fixed group contains already reserved command; no restart.'}}
            $prerequisites=if($name -ceq 'session-four'){@('sealed-basic','sealed-session-wrapper-enable')}else{@('sealed-session-join','sealed-native-review','sealed-controls-wrapper-enable','sealed-proposal-wrapper-enable')}
            foreach($c in $prerequisites){if(-not $used.ContainsKey($c)){throw 'Original review/install/join prerequisites not attempted; no group effect.'}}
            $usedGroups[$name]=$true
            Write-ControlNew (Join-Path $root ('group-'+$name+'-reserved.json')) ([ordered]@{Group=$name;Commands=$list;Confirmation=$parts[2];State='GROUP_RESERVED_NO_ADDITIONAL_AUTHORITY';NoHumanPause=$true;OriginalAuthorityMilliseconds=60000;NoTTLRefresh=$true;StopSchedulingBudgetMilliseconds=40000;NoReplay=$true})
            $group=@{Name=$name;Commands=$list;Index=0;Clock=[Diagnostics.Stopwatch]::StartNew()}
            # The ordinary once-only dispatch/poll branch executes every step unchanged.
        } elseif ($line -ceq 'finish-capture|CLOSE_ONLY_OBSERVATIONS_AFTER_TRUE_PROCESS_EXIT_AND_STREAM_EOF') {
            if ($hostUnknown -or $captureTerminal.Unknown -or $null -eq $lastGuestCapture) { throw 'Unknown capture cannot close.' }
            foreach($n in @('Out','Err','journal')) { Assert-C28CaptureClosed $lastGuestCapture[$n] $hostCapture[$n] }
            if ($null -eq $actualExit -or -not $rawEOF -or $null -eq $terminalJournalLength -or -not $journalCaughtUp -or $rawTotals['journal'] -ne $terminalJournalLength -or $pending -cne '') { throw 'No complete original terminal/readers/journal/pending observation; cannot finish capture.' }
            Assert-CapturedOriginalJournal $terminalJournalLength
            foreach ($f in $rawHandles.Values) { $f.Flush($true);$f.Dispose() };$rawHandles=@{}
            $hs=@{};foreach($n in @('Out','Err','journal')){$f=[IO.File]::Open((Join-Path $root ($n+'.log')),[IO.FileMode]::Open,[IO.FileAccess]::Read,[IO.FileShare]::Read);try{$hs[$n]=Get-PublicSha $f}finally{$f.Dispose()}}
            Write-ControlNew (Join-Path $root 'capture-final.json') ([ordered]@{GuestPid=$guestPid;ActualExit=$actualExit;RawSHA256=$hs;ActualReadersEOF=$true;FrozenTerminalJournalLength=$terminalJournalLength;ActualJournalBytes=$rawTotals['journal'];JournalParsed=$true;PendingCommand=$pending;Scope='CAPTURE_ONLY_REQUIRES_INDEPENDENT_PRODUCT_REVIEW';SDKComplete=$false;SystemRestorationGuaranteed=$false})
            break
        } else {
            $parts=$line.Split('|')
            if ($parts.Count -ne 3 -or $parts[0] -cne 'dispatch' -or -not $commandMap.ContainsKey($parts[1]) -or $parts[2] -cne $commandMap[$parts[1]] -or $used.ContainsKey($parts[1]) -or $pending -cne '' -or $attempt -ge 64 -or $null -ne $actualExit) { throw 'Fixed once-only confirmed command unavailable; no retry.' }
            $attempt++;$used[$parts[1]]=$true;$pending=$parts[1]
            Write-ControlNew (Join-Path $root ('attempt-{0:D2}.json' -f $attempt)) ([ordered]@{GuestPid=$guestPid;Command=$parts[1];ExactScopeConfirmation=$parts[2];State='RESERVED_UNKNOWN_BEFORE_SEND';NoReplay=$true;NoAuthorityGranted=$true})
            $result=@(Invoke-Command -Session $session -ArgumentList $parts[1],$parts[2] -ScriptBlock {param($c,$t) Dispatch-C28One $c $t} -ErrorAction Stop)
            if ($result.Count -ne 1 -or $result[0].Kind -cne 'DISPATCH_WRITTEN_NOT_APPLIED' -or [int]$result[0].Pid -ne $guestPid -or $result[0].Command -cne $parts[1]) { throw 'Dispatch outcome Unknown; never resend.' }
            Write-ControlNew (Join-Path $root ('delivery-{0:D2}.json' -f $attempt)) ([ordered]@{GuestPid=$guestPid;Command=$parts[1];Written=$true;Applied='NOT_PROVEN';NextCommand='WAIT_ORIGINAL_JOURNAL_OBSERVATION'})
        }
    }
} catch {
    if ($started -or $installationReserved) { Park-Unknown }
    [Console]::Error.WriteLine('Blocked/pre-start failure; private details not recorded. No retry.');exit 1
} finally {
    # Only pre-start failure or actual process exit reaches here. Park retains everything.
    $taskCredential=$null
    foreach($x in @($taskSecurePassword)){if($null -ne $x){$x.Dispose()}}
    foreach($f in $rawHandles.Values){$f.Dispose()};foreach($f in $pins){$f.Dispose()}
    if(Get-Variable ManifestPinStream -Scope Script -ErrorAction SilentlyContinue){$script:ManifestPinStream.Dispose()}
    # No Remove-PSSession/Close guest stdin/kill/cleanup or reconstruction of any owner.
}
