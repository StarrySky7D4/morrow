throw 'SOURCE_ONLY_DISABLED_STAGE19_ADAPTER003_BINDING_PENDING_NO_VM_OR_CREDENTIALS'
# Definitions only; pending source constants cannot grant once authority.
$C28Stage19Binding=@{
    State='PENDING_REAL_PRODUCTION_MANIFEST_GUEST_AND_ONCE_AUTHORITY'
    OperationId='7ba550b9-ad7f-499b-a56f-9711d9560856'
    PublicManifestSHA256='PENDING_REAL_THREE_PE_MANIFEST_SHA256'
    HarnessSHA256='PENDING_REAL_STAGE18_HARNESS_SHA256'
    HarnessBytes=0L
    RunnerSHA256='PENDING_FRESH_ORIGINAL_RUNNER_SHA256'
    RunnerBytes=0L
    SetupSHA256='PENDING_FRESH_ORIGINAL_SETUP_SHA256'
    SetupBytes=0L
    HostVmId='PENDING_EXPLICIT_HOST_VM_ID'
    GuestFirmwareUuid='PENDING_EXPLICIT_GUEST_IDENTITY'
    GuestPowerShellSHA256='PENDING_ACTUAL_GUEST_POWERSHELL_SHA256'
    GuestWhoamiSHA256='PENDING_ACTUAL_GUEST_WHOAMI_SHA256'
    GuestWhoamiBytes=0L
    PrivateCallerName='PENDING_EXPLICIT_PRIVATE_CALLER_NAME'
    OnceAuthorizationSHA256='PENDING_SEPARATE_EXPLICIT_ONCE_AUTHORITY'
}
function Assert-C28Stage19Binding {
    $b=$script:C28Stage19Binding
    if ($b.State -cne 'ROOT_REVIEWED_BOUND_AND_EXPLICIT_ONCE_AUTHORIZED' -or $b.OperationId -cne '7ba550b9-ad7f-499b-a56f-9711d9560856') { throw 'Stage19 artifact/guest/authority binding PENDING.' }
    foreach($name in @('PublicManifestSHA256','HarnessSHA256','RunnerSHA256','SetupSHA256','GuestPowerShellSHA256','GuestWhoamiSHA256','OnceAuthorizationSHA256')) { if ([string]$b[$name] -cnotmatch '^[0-9a-f]{64}$') { throw 'Missing real source/authority pin.' } }
    foreach($name in @('HarnessBytes','RunnerBytes','SetupBytes','GuestWhoamiBytes')) { if ([long]$b[$name] -le 0 -or [long]$b[$name] -gt 134217728) { throw 'Missing real bounded public PE size.' } }
    foreach($name in @('HostVmId','GuestFirmwareUuid')) { $id=[Guid]::Empty;if(-not [Guid]::TryParse([string]$b[$name],[ref]$id) -or $id -eq [Guid]::Empty){throw 'Actual approved VM/guest identity absent.'} }
    if ([string]::IsNullOrWhiteSpace($b.PrivateCallerName) -or $b.PrivateCallerName.StartsWith('PENDING') -or $b.PrivateCallerName.Length -gt 256) { throw 'Explicit private caller identity absent.' }
}
function Read-C28ManualPrivateCredential([string]$ExpectedUser) {
    Assert-C28Stage19Binding
    if ($ExpectedUser -cne $script:C28Stage19Binding.PrivateCallerName) { throw 'Caller identity differs; no credential prompt.' }
    $credential=Get-Credential -UserName $ExpectedUser -Message 'Manually supply the separately authorized Stage19 private VM credential. It is never recorded.'
    if ($null -eq $credential -or $credential.UserName -cne $ExpectedUser -or $credential.Password.Length -eq 0) { throw 'Explicit manual private credential required.' }
    return $credential
}
# Definitions only. Windows PowerShell 5.1; no invocation when loaded.
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
function Get-PublicSha([IO.Stream]$Stream) {
    $h = [Security.Cryptography.SHA256]::Create()
    try { return [BitConverter]::ToString($h.ComputeHash($Stream)).Replace('-', '').ToLowerInvariant() }
    finally { $h.Dispose() }
}
function Assert-PlainPath([string]$Path) {
    if ($Path -notmatch '^C:\\' -or $Path -match '(?:^|\\)\.\.?($|\\)' -or $Path.Substring(2).Contains(':')) { throw 'Fixed plain C drive path required.' }
    $cursor = $Path
    while ($cursor) {
        if ([IO.File]::Exists($cursor) -or [IO.Directory]::Exists($cursor)) {
            if (([IO.File]::GetAttributes($cursor) -band [IO.FileAttributes]::ReparsePoint) -ne 0) { throw 'Reparse path rejected.' }
        } else {
            try { [void][IO.File]::GetAttributes($cursor); throw 'Unexpected path object.' }
            catch [IO.FileNotFoundException] {} catch [IO.DirectoryNotFoundException] {}
        }
        $cursor = [IO.Path]::GetDirectoryName($cursor.TrimEnd('\'))
    }
}
function Write-PublicNew([string]$Path, $Value) {
    $raw = [Text.UTF8Encoding]::new($false).GetBytes(($Value | ConvertTo-Json -Depth 6 -Compress) + "`n")
    if ($raw.Length -gt 262144) { throw 'Public record bound.' }
    $f = [IO.File]::Open($Path, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::Read)
    try { $f.Write($raw, 0, $raw.Length); $f.Flush($true) } finally { $f.Dispose() }
}
function Read-FixedManifest {
    $path = Join-Path $PSScriptRoot 'public-manifest.json'
    $f = [IO.File]::Open($path, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::Read)
    try {
        if ($f.Length -gt 16384 -or (Get-PublicSha $f) -cne 'PENDING_REAL_THREE_PE_MANIFEST_SHA256') { throw 'Public manifest pin mismatch.' }
        $value = [Text.Encoding]::UTF8.GetString([IO.File]::ReadAllBytes($path)) | ConvertFrom-Json
        $script:ManifestPinStream = $f
        return $value
    } catch { $f.Dispose(); throw }
}
function New-FixedConnection([Management.Automation.PSCredential]$PrivateCredential) {
    if ($PSVersionTable.PSVersion.Major -ne 5 -or $PSVersionTable.PSVersion.Minor -ne 1) { throw 'Windows PowerShell 5.1 required.' }
    $id = [Guid]'PENDING_EXPLICIT_HOST_VM_ID'
    $vm = Get-VM -Id $id -ErrorAction Stop
    if ($vm.Id -ne $id -or $vm.State.ToString() -cne 'Running') { throw 'Fixed running VM required; no start/configuration.' }
    if ($null -eq $PrivateCredential -or $PrivateCredential.UserName -cne 'PENDING_EXPLICIT_PRIVATE_CALLER_NAME') { throw 'Fixed private caller credential required; no credential input in public stage.' }
    return New-PSSession -VMId $id -Credential $PrivateCredential -ErrorAction Stop
}
# This exact remote definition returns only fixed sanitized scalars. It neither
# reads credentials/keys nor starts/stops services. Caller must explicitly invoke.
$FixedGuestCheck = {
    Set-StrictMode -Version Latest
    $ErrorActionPreference = 'Stop'
    function global:Assert-GuestPath([string]$p) {
        if ($p -notmatch '^C:\\' -or $p -match '(?:^|\\)\.\.?($|\\)' -or $p.Substring(2).Contains(':')) { throw 'Guest path rejected.' }
        $c = $p
        while ($c) {
            try { if (([IO.File]::GetAttributes($c) -band [IO.FileAttributes]::ReparsePoint) -ne 0) { throw 'Guest reparse rejected.' } }
            catch [IO.FileNotFoundException] {} catch [IO.DirectoryNotFoundException] {}
            $c = [IO.Path]::GetDirectoryName($c.TrimEnd('\'))
        }
    }
    function global:Assert-GuestFreshAttempt {
        foreach ($p in @('C:\MorrowSdkQualification\C28-stage19-once001\attempt-001','C:\MorrowSdkQualification\C28-stage19-once001\attempt-001.evidence')) {
            Assert-GuestPath $p
            try { [void][IO.File]::GetAttributes($p); throw 'Attempt/evidence collision; no adoption.' }
            catch [IO.FileNotFoundException] {} catch [IO.DirectoryNotFoundException] {}
        }
    }
    function global:Get-GuestSha([IO.Stream]$s) {
        $h = [Security.Cryptography.SHA256]::Create()
        try { [BitConverter]::ToString($h.ComputeHash($s)).Replace('-', '').ToLowerInvariant() } finally { $h.Dispose() }
    }
    $s = Get-CimInstance -ClassName Win32_ComputerSystem
    $p = Get-CimInstance -ClassName Win32_ComputerSystemProduct
    $identity = [Security.Principal.WindowsIdentity]::GetCurrent()
    try { $admin = [Security.Principal.WindowsPrincipal]::new($identity).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator) }
    finally { $identity.Dispose() }
    if ($s.Manufacturer -cne 'Microsoft Corporation' -or $s.Model -cne 'Virtual Machine' -or -not $s.HypervisorPresent -or ([string]$p.UUID).ToLowerInvariant() -cne 'PENDING_EXPLICIT_GUEST_IDENTITY' -or -not $admin -or -not [Environment]::Is64BitProcess -or -not [Environment]::Is64BitOperatingSystem) { throw 'Fresh fixed guest/Admin64 identity mismatch.' }
    Assert-GuestPath 'C:\MorrowSdkQualification\C28-stage19-once001\bin'
    Assert-GuestFreshAttempt
    if (@(Get-LocalUser | Where-Object { $_.Name -in @('CodexSandboxOffline','CodexSandboxOnline') }).Count -ne 0 -or @(Get-LocalGroup | Where-Object Name -eq 'CodexSandboxUsers').Count -ne 0 -or (Test-Path -LiteralPath 'Registry::HKEY_LOCAL_MACHINE\SOFTWARE\OpenAI\Codex\WindowsSandboxService')) { throw 'Fixed account/group/registration collision.' }
    [pscustomobject]@{ Schema='morrow-c28-fresh-guest-check-v1'; FirmwareUuid='PENDING_EXPLICIT_GUEST_IDENTITY'; Administrator=$true; Process64=$true; AttemptAbsent=$true; FixedCollisionFree=$true }
}
