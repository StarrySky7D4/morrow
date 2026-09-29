param([Parameter(Mandatory=$true)][string]$RunId)
$ErrorActionPreference = 'Stop'
# Adapted from frozen peer prepare_materials.ps1 (16eb4227...), with all writes
# restricted to this joint batch. Existing directory permissions are never changed.
if ($RunId -notmatch '^[A-Za-z0-9_-]{1,64}$') { throw 'Invalid run ID' }
$jointBatch = [IO.Path]::GetFullPath($PSScriptRoot)
$runsPath = [IO.Path]::GetFullPath((Join-Path $jointBatch 'runs'))
$runPath = [IO.Path]::GetFullPath((Join-Path $runsPath $RunId))
if (-not $runPath.StartsWith($runsPath + [IO.Path]::DirectorySeparatorChar,[StringComparison]::OrdinalIgnoreCase)) { throw 'Outside batch' }
$ancestor = Get-Item -LiteralPath $jointBatch
while ($null -ne $ancestor) {
    if (($ancestor.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) { throw 'Reparse ancestor rejected' }
    $ancestor = $ancestor.Parent
}
if (Test-Path -LiteralPath $runPath) { throw 'Fresh run required' }
[IO.Directory]::CreateDirectory($runsPath) | Out-Null
New-Item -ItemType Directory -Path $runPath -ErrorAction Stop | Out-Null
$currentSid = [Security.Principal.WindowsIdentity]::GetCurrent().User
$systemSid = [Security.Principal.SecurityIdentifier]::new('S-1-5-18')
$security = [Security.AccessControl.DirectorySecurity]::new()
$security.SetOwner($currentSid)
$security.SetAccessRuleProtection($true,$false)
$inherit = [Security.AccessControl.InheritanceFlags]'ContainerInherit,ObjectInherit'
foreach ($sid in @($currentSid,$systemSid)) {
    $rule = [Security.AccessControl.FileSystemAccessRule]::new($sid,'FullControl',$inherit,'None','Allow')
    $security.AddAccessRule($rule)
}
Set-Acl -LiteralPath $runPath -AclObject $security
$actual = Get-Acl -LiteralPath $runPath
if (-not $actual.AreAccessRulesProtected) { throw 'Unprotected ACL' }
$rules = $actual.GetAccessRules($true,$true,[Security.Principal.SecurityIdentifier])
if ($rules.Count -ne 2) { throw 'Expected exactly two ACEs' }
foreach ($rule in $rules) {
    if ($rule.IdentityReference.Value -notin @($currentSid.Value,$systemSid.Value) -or $rule.AccessControlType -ne 'Allow' -or $rule.FileSystemRights -ne 'FullControl' -or $rule.InheritanceFlags -ne $inherit) { throw 'Unexpected ACE' }
}
$receipt = [ordered]@{status='private_joint_run_ready_no_authority';root=$runPath;acl_sddl=$actual.Sddl;raw_frames_are_test_material=$true;tool_sha256=(Get-FileHash -LiteralPath $PSCommandPath -Algorithm SHA256).Hash.ToLowerInvariant()}
$json = $receipt | ConvertTo-Json -Depth 4
[IO.File]::WriteAllText((Join-Path $runPath 'acl-receipt.json'),$json+[Environment]::NewLine,[Text.UTF8Encoding]::new($false))
$json
