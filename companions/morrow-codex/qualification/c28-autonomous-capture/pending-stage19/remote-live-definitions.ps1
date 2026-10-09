throw 'SOURCE_ONLY_DISABLED_STAGE19_ADAPTER003_PENDING_NO_VM_OR_CREDENTIAL_ENTRY'
# Definitions are installed only in the one newly acquired fixed PSSession.
# Live objects remain there; only bounded scalar/byte records cross the boundary.
$InstallLiveDefinitions = {
    Set-StrictMode -Version Latest
    $ErrorActionPreference='Stop'
    if (Get-Variable -Name C28Live -Scope Global -ErrorAction SilentlyContinue) { throw 'Remote controller already exists; no adoption or second process.' }
    $global:C28Live=$null
    $global:C28CommandMap=@{
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
    function global:Assert-C28Stage19RemoteBinding {
        if ('PENDING_REAL_PRODUCTION_MANIFEST_GUEST_AND_ONCE_AUTHORITY' -cne 'ROOT_REVIEWED_BOUND_AND_EXPLICIT_ONCE_AUTHORIZED') { throw 'Stage19 remote source remains PENDING; no VM observation or Start.' }
        foreach($sha in @('PENDING_REAL_THREE_PE_MANIFEST_SHA256','PENDING_REAL_STAGE18_HARNESS_SHA256','PENDING_FRESH_ORIGINAL_RUNNER_SHA256','PENDING_FRESH_ORIGINAL_SETUP_SHA256','PENDING_ACTUAL_GUEST_POWERSHELL_SHA256','PENDING_ACTUAL_GUEST_WHOAMI_SHA256','PENDING_SEPARATE_EXPLICIT_ONCE_AUTHORITY')) { if($sha -cnotmatch '^[0-9a-f]{64}$'){throw 'Real production/authority pin absent.'} }
        $id=[Guid]::Empty;if(-not [Guid]::TryParse('PENDING_EXPLICIT_GUEST_IDENTITY',[ref]$id) -or $id -eq [Guid]::Empty){throw 'Explicit actual guest identity absent.'}
    }
    function global:Assert-C28IdentityAndCollisions {
        $s=Get-CimInstance Win32_ComputerSystem;$p=Get-CimInstance Win32_ComputerSystemProduct
        $i=[Security.Principal.WindowsIdentity]::GetCurrent()
        try { $a=[Security.Principal.WindowsPrincipal]::new($i).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator) } finally { $i.Dispose() }
        if ($s.Manufacturer -cne 'Microsoft Corporation' -or $s.Model -cne 'Virtual Machine' -or -not $s.HypervisorPresent -or ([string]$p.UUID).ToLowerInvariant() -cne 'PENDING_EXPLICIT_GUEST_IDENTITY' -or -not $a -or -not [Environment]::Is64BitProcess) { throw 'Fixed guest/Admin identity changed.' }
        if (@(Get-LocalUser | Where-Object { $_.Name -in @('CodexSandboxOffline','CodexSandboxOnline') }).Count -ne 0 -or @(Get-LocalGroup | Where-Object Name -eq 'CodexSandboxUsers').Count -ne 0 -or (Test-Path -LiteralPath 'Registry::HKEY_LOCAL_MACHINE\SOFTWARE\OpenAI\Codex\WindowsSandboxService')) { throw 'Fresh setup account/group/registration collision; no adoption.' }
        foreach ($n in @('BFE','MpsSvc','vmicvmsession')) { if ((Get-Service -Name $n -ErrorAction Stop).Status.ToString() -cne 'Running') { throw 'Required existing service not running; no policy/service fix.' } }
    }
    function global:Read-C28Journal {
        $v=$global:C28Live
        $path='C:\MorrowSdkQualification\C28-stage19-once001\attempt-001.evidence\journal.jsonl'
        Assert-GuestPath $path
        if (-not [IO.File]::Exists($path)) { return ,@() }
        $f=[IO.File]::Open($path,[IO.FileMode]::Open,[IO.FileAccess]::Read,[IO.FileShare]::ReadWrite)
        try {
            if ($f.Length -gt 33554432) { throw 'Fixed journal byte bound.' }
            $raw=New-Object byte[] ([int]$f.Length);$n=0
            while ($n -lt $raw.Length) { $r=$f.Read($raw,$n,$raw.Length-$n);if ($r -eq 0) { throw 'Journal read shortened; Unknown.' };$n+=$r }
        } finally { $f.Dispose() }
        $last=-1
        for ($j=$raw.Length-1;$j -ge 0;$j--) { if ($raw[$j] -eq 10) { $last=$j;break } }
        if ($last -lt 0) { return ,@() }
        $text=[Text.UTF8Encoding]::new($false,$true).GetString($raw,0,$last+1)
        $rows=@();$expected=0
        foreach ($line in $text.Split([char]10)) {
            if ($line.Length -eq 0) { continue }
            if ([Text.Encoding]::UTF8.GetByteCount($line) -gt 131072 -or $expected -ge 256) { throw 'Journal record bound.' }
            $row=$line | ConvertFrom-Json
            if ([int]$row.sequence -ne $expected) { throw 'Original journal sequence mismatch; no restoration.' }
            $rows+=,$row;$expected++
        }
        return ,$rows
    }
    function global:Update-C28Pending {
        $v=$global:C28Live
        if ($null -eq $v.Pending) { return }
        $rows=Read-C28Journal
        $rseq=$null
        foreach ($r in $rows) {
            if ([int]$r.sequence -lt $v.Pending.BeforeSequence) { continue }
            if ($v.Pending.Command -ceq 'begin' -and $r.kind -ceq 'guest-identity-validated') { $v.Pending=$null;return }
            if ($v.Pending.Command -ceq 'exit' -and $r.kind -ceq 'FINISHED_EXPLICIT_WORKFLOW_NOT_FULL_ACCEPTANCE') { $v.Pending=$null;return }
            if ($r.kind -ceq 'ATTEMPT_RESERVED_OUTCOME_UNKNOWN') {
                $step=[string]$r.payload.step
                $exact=$step -ceq $v.Pending.Command
                $cleanup=$v.Pending.Command -in @('sealed-cleanup','sealed-repair-cleanup') -and $step -match ('^'+[regex]::Escape($v.Pending.Command)+'-[1-4]$')
                $reap=$v.Pending.Command -ceq 'reap-factory-scheduler' -and $step -match '^reap-factory-scheduler-[1-4]$'
                if ($exact -or $cleanup -or $reap) { $rseq=[int]$r.sequence }
            }
            if ($null -ne $rseq -and [int]$r.sequence -gt $rseq) {
                if ($r.kind -ceq 'ORIGINAL_SEALED_STEP_RETURNED_NOT_FULL_ACCEPTANCE' -and $v.Pending.Command.StartsWith('sealed-')) { $v.Pending=$null;return }
                if ($r.kind -ceq 'STEP_RETURNED_OK_NOT_FULL_ACCEPTANCE' -and [string]$r.payload -ceq $v.Pending.Command) { $v.Pending=$null;return }
                if ($r.kind -in @('UNKNOWN_OR_REJECTED_NO_REPLAY','SEALED_UNKNOWN_OR_REJECTED_NO_REPLAY')) { $v.Blocked=$true;$v.Pending=$null;return }
            }
        }
    }
    function global:Start-C28OneProcess([byte[]]$ManifestBytes) {
        if ($null -ne $global:C28Live) { throw 'Start already reserved; no retry/adoption.' }
        Assert-C28Stage19RemoteBinding
        Assert-C28IdentityAndCollisions;Assert-GuestFreshAttempt
        if ($ManifestBytes.Length -gt 16384) { throw 'Fixed manifest byte bound.' }
        $memory=[IO.MemoryStream]::new($ManifestBytes)
        try { if ((Get-GuestSha $memory) -cne 'PENDING_REAL_THREE_PE_MANIFEST_SHA256') { throw 'Manifest identity mismatch.' } } finally { $memory.Dispose() }
        $m=[Text.UTF8Encoding]::new($false,$true).GetString($ManifestBytes) | ConvertFrom-Json
        if (@($m.files).Count -ne 3) { throw 'Exactly three public images required.' }
        $v=@{OperationId='7ba550b9-ad7f-499b-a56f-9711d9560856';Installation=$null;Process=$null;Pump=$null;Started=$false;Terminal=$false;Exit=$null;Pins=@();Pending=$null;Blocked=$false;CaptureFailed=$false;Used=@{};Attempts=0;Polls=0;TerminalJournalLength=$null}
        $global:C28Live=$v # Reserve the only actual object before any Start effect.
        $names=@('morrow-protected-checked-vm-harness.exe','codex-command-runner.exe','codex-windows-sandbox-setup.exe')
        $hashes=@('PENDING_REAL_STAGE18_HARNESS_SHA256','PENDING_FRESH_ORIGINAL_RUNNER_SHA256','PENDING_FRESH_ORIGINAL_SETUP_SHA256')
        $sizes=@(0L,0L,0L) # PENDING_REAL_PUBLIC_PE_SIZES; bind from actual artifacts, never infer.
        for ($i=0;$i -lt 3;$i++) {
            $e=$m.files[$i]
            if ($e.name -cne $names[$i] -or $e.sha256 -cne $hashes[$i] -or $e.bytes -ne $sizes[$i]) { throw 'Fixed three-image manifest roles differ.' }
            $path='C:\MorrowSdkQualification\C28-stage19-once001\bin\'+$names[$i];Assert-GuestPath $path
            $f=[IO.File]::Open($path,[IO.FileMode]::Open,[IO.FileAccess]::Read,[IO.FileShare]::Read);$v.Pins+=,$f
            if ($f.Length -ne $sizes[$i] -or $f.Length -gt 134217728 -or (Get-GuestSha $f) -cne $hashes[$i]) { throw 'Full guest image identity mismatch.' }
            $f.Position=0;$h=New-Object byte[] 64;if ($f.Read($h,0,64) -ne 64 -or $h[0] -ne 77 -or $h[1] -ne 90) { throw 'MZ identity missing.' }
            $off=[BitConverter]::ToInt32($h,60);if ($off -lt 64 -or $off -gt $f.Length-26) { throw 'PE header bound.' }
            $f.Position=$off;$h=New-Object byte[] 26;if ($f.Read($h,0,26) -ne 26 -or [BitConverter]::ToUInt32($h,0) -ne 17744 -or [BitConverter]::ToUInt16($h,4) -ne 34404 -or ([BitConverter]::ToUInt16($h,22) -band 8192) -ne 0 -or [BitConverter]::ToUInt16($h,24) -ne 523) { throw 'Actual AMD64 PE32+ nonDLL required.' }
        }
        foreach ($sys in @(@('C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe','PENDING_ACTUAL_GUEST_POWERSHELL_SHA256',0),@('C:\Windows\System32\whoami.exe','PENDING_ACTUAL_GUEST_WHOAMI_SHA256',0))) {
            Assert-GuestPath $sys[0];$f=[IO.File]::Open($sys[0],[IO.FileMode]::Open,[IO.FileAccess]::Read,[IO.FileShare]::Read);$v.Pins+=,$f
            if ((Get-GuestSha $f) -cne $sys[1] -or ($sys[2] -gt 0 -and $f.Length -ne $sys[2])) { throw 'Fresh fixed system file pin differs; not executed by this check.' }
        }
        $mf=[IO.File]::Open('C:\MorrowSdkQualification\C28-stage19-once001\bin\public-manifest.json',[IO.FileMode]::Open,[IO.FileAccess]::Read,[IO.FileShare]::Read);$v.Pins+=,$mf
        if ((Get-GuestSha $mf) -cne 'PENDING_REAL_THREE_PE_MANIFEST_SHA256') { throw 'Transferred manifest pin differs.' }
        $info=[Diagnostics.ProcessStartInfo]::new()
        $info.FileName='C:\MorrowSdkQualification\C28-stage19-once001\bin\morrow-protected-checked-vm-harness.exe'
        $info.Arguments='--phase live-guest-interactive --guest-root C:\MorrowSdkQualification\C28-stage19-once001\attempt-001 --runner C:\MorrowSdkQualification\C28-stage19-once001\bin\codex-command-runner.exe --runner-sha256 PENDING_FRESH_ORIGINAL_RUNNER_SHA256 --setup C:\MorrowSdkQualification\C28-stage19-once001\bin\codex-windows-sandbox-setup.exe --setup-sha256 PENDING_FRESH_ORIGINAL_SETUP_SHA256 --helper C:\MorrowSdkQualification\C28-stage19-once001\bin\morrow-protected-checked-vm-harness.exe --helper-sha256 PENDING_REAL_STAGE18_HARNESS_SHA256 --inventory-shell C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe --inventory-shell-sha256 PENDING_ACTUAL_GUEST_POWERSHELL_SHA256 --expected-guest-uuid PENDING_EXPLICIT_GUEST_IDENTITY --sid-query C:\Windows\System32\whoami.exe --sid-query-sha256 PENDING_ACTUAL_GUEST_WHOAMI_SHA256'
        $info.WorkingDirectory='C:\MorrowSdkQualification\C28-stage19-once001\bin';$info.UseShellExecute=$false;$info.CreateNoWindow=$true
        $info.RedirectStandardInput=$true;$info.RedirectStandardOutput=$true;$info.RedirectStandardError=$true
        $v.Process=[Diagnostics.Process]::new();$v.Process.StartInfo=$info
        # Only the explicit once-installed retained Assembly/type may construct this pump.
        $v.Installation=Assert-C28InstalledPump '7ba550b9-ad7f-499b-a56f-9711d9560856' 'PENDING_REAL_THREE_PE_MANIFEST_SHA256' 'PENDING_SEPARATE_EXPLICIT_ONCE_AUTHORITY'
        try { $v.Pump=[Activator]::CreateInstance($v.Installation.PumpType,[object[]]@('C:\MorrowSdkQualification\C28-stage19-once001\attempt-001.evidence\journal.jsonl')) }
        catch { $v.CaptureFailed=$true;throw 'Reviewed pump construction failed; same reservation retained, no Start/retry.' }
        try { $v.Pump.Prepare() } catch { $v.CaptureFailed=$true;throw 'PreStart pump preparation fault; original owner retained, no retry.' }
        try { $didStart=$v.Process.Start() }
        catch {
            $v.CaptureFailed=$true
            # Same-object capture only. No successful Start is inferred from this attempt.
            # This catch always throws, so the successful-Start Attach below is unreachable.
            try { $v.Pump.Attach($v.Process) } catch { }
            throw 'Start outcome Unknown; original Process and prepared pump retained, no retry or automatic cancellation.'
        }
        if (-not $didStart) {
            $v.CaptureFailed=$true
            $v.Pump.CancelBeforeAttach() # Only the explicit false return proves no child was started.
            throw 'Actual Start returned false; no retry.'
        }
        $v.Started=$true
        # The only successful Start is handed to the already-prepared pump exactly once.
        # Attach retains this Process before accessing its redirected streams.
        try { $v.Pump.Attach($v.Process) }
        catch { $v.CaptureFailed=$true;throw 'PostStart attach Unknown; original Process and pump retained.' }
        try { $v.Process.StandardInput.AutoFlush=$true }
        catch { $v.CaptureFailed=$true;throw 'PostAttach observer initialization Unknown; autonomous pump remains retained.' }
        [pscustomobject]@{Kind='STARTED';OperationId='7ba550b9-ad7f-499b-a56f-9711d9560856';Pid=$v.Process.Id;Mode='SAME_ORIGINAL_LIVE_HARNESS_NOT_ACCEPTANCE'}
    }
    function global:Assert-C28PumpDispatchable($v) {
        # Non-emitting status only; never Snapshot with invented ACKs to inspect health.
        try {
            if ($v.OperationId -cne '7ba550b9-ad7f-499b-a56f-9711d9560856' -or $null -eq $v.Installation -or $null -eq $v.Pump -or -not [Object]::ReferenceEquals($v.Pump.GetType().Assembly,$v.Installation.Assembly)) { throw 'Original reviewed pump operation/type identity changed.' }
            if ($null -eq $v.Pump -or $v.Pump.CaptureFailed) { throw 'Current autonomous capture is Unknown.' }
            if ($v.Terminal -or $v.Process.HasExited) { $v.Terminal=$true;throw 'Original process is terminal; no command.' }
        } catch { $v.CaptureFailed=$true;throw 'Current pump/process observation rejects dispatch; original owner retained.' }
    }
    function global:Dispatch-C28One([string]$Command,[string]$Confirmation) {
        $v=$global:C28Live
        if ($null -eq $v -or -not $v.Started -or $v.Terminal -or $v.CaptureFailed) { throw 'No usable original live process; no adoption.' }
        Assert-C28PumpDispatchable $v
        Update-C28Pending
        if ($null -ne $v.Pending) { throw 'Original attempt still pending; no next command.' }
        if (-not $global:C28CommandMap.ContainsKey($Command) -or $Confirmation -cne $global:C28CommandMap[$Command] -or $v.Used.ContainsKey($Command) -or $v.Attempts -ge 64) { throw 'Fixed exact once-only command/confirmation required.' }
        if ($v.Blocked -and $Command -notin @('sealed-cleanup','sealed-repair-cleanup','release-factory','reap-factory-scheduler','finish-owner','prepare-cleanup','finish-cleanup','exit')) { throw 'Original Unknown permits explicit same-object cleanup only.' }
        if ($Command -ceq 'setup') { Assert-C28IdentityAndCollisions }
        if ($Command -cne 'begin' -and -not $v.Used.ContainsKey('begin')) { throw 'Actual identity begin must precede business.' }
        $rows=Read-C28Journal
        Assert-C28PumpDispatchable $v # Recheck immediately before the unchanged once-only reservation/write.
        $v.Used[$Command]=$true;$v.Attempts++
        $v.Pending=@{Command=$Command;BeforeSequence=@($rows).Count}
        # Reservation precedes either write: partial command/confirmation is Unknown, never resent.
        if ($Command -ceq 'begin') { $v.Process.StandardInput.WriteLine($Confirmation) }
        elseif ($Command -ceq 'exit') { $v.Process.StandardInput.WriteLine('exit') }
        else { $v.Process.StandardInput.WriteLine($Command);$v.Process.StandardInput.WriteLine($Confirmation) }
        $v.Process.StandardInput.Flush()
        [pscustomobject]@{Kind='DISPATCH_WRITTEN_NOT_APPLIED';Pid=$v.Process.Id;Command=$Command;Attempt=$v.Attempts}
    }
    function global:Poll-C28One([long]$AckOut,[long]$AckErr,[long]$AckJournal,[int]$DrainMask) {
        $v=$global:C28Live
        if ($null -eq $v -or -not $v.Started -or $null -eq $v.Pump -or $v.Polls -ge 4096 -or $DrainMask -lt 0 -or $DrainMask -gt 7) { throw 'Original process/observation scope unavailable.' }
        $v.Polls++ # Same hard observation ceiling; draining does not depend on this RPC.
        try { $s=$v.Pump.Snapshot($AckOut,$AckErr,$AckJournal,$DrainMask) }
        catch { $v.CaptureFailed=$true;throw 'Pump snapshot Unknown; same autonomous owner retained.' }
        if ($s.CaptureFailed) { $v.CaptureFailed=$true }
        if ($s.Terminal) {
            if ($null -eq $s.ActualExitCode) { $v.CaptureFailed=$true;throw 'Terminal snapshot lacks actual exit.' }
            if ($null -ne $v.Exit -and $v.Exit -ne $s.ActualExitCode) { $v.CaptureFailed=$true;throw 'Original exit changed.' }
            $v.Terminal=$true;$v.Exit=[int]$s.ActualExitCode
            [pscustomobject]@{Kind='TERMINAL';Pid=$v.Process.Id;ActualExit=$v.Exit}
        }
        foreach ($frame in $s.Frames) {
            if ($frame.Stream -cnotin @('Out','Err','journal') -or $frame.Offset -lt 0 -or $null -eq $frame.Bytes -or $frame.Bytes.Length -lt 1 -or $frame.Bytes.Length -gt 16384) { $v.CaptureFailed=$true;throw 'Fixed pump frame shape differs.' }
            [pscustomobject]@{Kind='CAPTURE_BYTES';Pid=$v.Process.Id;Stream=[string]$frame.Stream;Offset=[long]$frame.Offset;Bytes=[Convert]::ToBase64String($frame.Bytes)}
        }
        # Preserve the original journal/business observer. It never drives capture reads.
        # Its failure cannot stop the independent pipe readers or journal worker.
        if (-not $v.CaptureFailed) {
            try { Update-C28Pending } catch { $v.CaptureFailed=$true }
        }
        $rows=@{}
        foreach ($n in @('Out','Err','journal')) {
            $c=if($n -ceq 'Out'){$s.Out}elseif($n -ceq 'Err'){$s.Err}else{$s.Journal}
            if ($c.Unknown) { $v.CaptureFailed=$true }
            $rows[$n]=@{Observed=[long]$c.Observed;Emitted=[long]$c.Emitted;SavedAck=[long]$c.SavedAck;EOF=[bool]$c.EOF;DrainOnly=[bool]$c.DrainOnly;Unknown=[bool]$c.Unknown;Discarded=[long]$c.Discarded;Retained=[long]$c.Retained}
        }
        $v.TerminalJournalLength=$s.TerminalJournalLength
        [pscustomobject]@{Kind='OBSERVATION';Pid=$v.Process.Id;PendingCommand=$(if($null -eq $v.Pending){''}else{$v.Pending.Command});Blocked=[bool]$v.Blocked;CaptureFailed=[bool]$v.CaptureFailed;Terminal=[bool]$s.Terminal;ReadersActuallyEOF=[bool]$s.ReadersEOF;JournalBytes=[long]$s.Journal.Observed;TerminalJournalLength=$s.TerminalJournalLength;JournalCaughtUp=([bool]$s.JournalCaughtUp -and [bool]$s.Terminal -and [bool]$s.Journal.EOF -and -not [bool]$s.Journal.Unknown -and $null -ne $s.TerminalJournalLength -and $s.Journal.SavedAck -eq $s.TerminalJournalLength);Capture=$rows;PumpReadersJoined=[bool]$s.ReadersJoined}
        # No reader task, process wait, file tail, Dispose, stdin.Close, Kill or reconnect.
        # PumpReadersJoined describes only pump threads; never Workbench/native join.
    }
}
