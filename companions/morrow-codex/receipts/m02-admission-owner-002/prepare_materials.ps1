$ErrorActionPreference = 'Stop'
# Fixed batch scope; never alters permissions of existing directories.
$pluginRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..\..'))
$basePath = Join-Path $pluginRoot 'out\m02-admission-owner-002\private-runs'
$existingAncestor = $basePath
while (-not (Test-Path -LiteralPath $existingAncestor)) { $existingAncestor = Split-Path -Parent $existingAncestor }
$candidateAncestor = Get-Item -LiteralPath $existingAncestor
while ($null -ne $candidateAncestor) {
    if (($candidateAncestor.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) { throw 'Reparse ancestor rejected' }
    $candidateAncestor = $candidateAncestor.Parent
}
[IO.Directory]::CreateDirectory($basePath) | Out-Null
$materialRoot = Join-Path $basePath ([Guid]::NewGuid().ToString('N'))
if (Test-Path -LiteralPath $materialRoot) { throw 'Fresh materials path required' }
[IO.Directory]::CreateDirectory($materialRoot) | Out-Null
$currentSid = [Security.Principal.WindowsIdentity]::GetCurrent().User
$systemSid = [Security.Principal.SecurityIdentifier]::new('S-1-5-18')
$security = [Security.AccessControl.DirectorySecurity]::new()
$security.SetOwner($currentSid)
$security.SetAccessRuleProtection($true, $false)
$inherit = [Security.AccessControl.InheritanceFlags]'ContainerInherit,ObjectInherit'
foreach ($sid in @($currentSid, $systemSid)) {
    $rule = [Security.AccessControl.FileSystemAccessRule]::new($sid, 'FullControl', $inherit, 'None', 'Allow')
    $security.AddAccessRule($rule)
}
Set-Acl -LiteralPath $materialRoot -AclObject $security
$actual = Get-Acl -LiteralPath $materialRoot
if (-not $actual.AreAccessRulesProtected) { throw 'Materials ACL is not protected' }
$rules = $actual.GetAccessRules($true, $true, [Security.Principal.SecurityIdentifier])
if ($rules.Count -ne 2) { throw 'Expected exactly two materials ACEs' }
foreach ($rule in $rules) {
    if ($rule.IdentityReference.Value -notin @($currentSid.Value, $systemSid.Value) -or $rule.AccessControlType -ne 'Allow' -or $rule.FileSystemRights -ne 'FullControl' -or $rule.InheritanceFlags -ne $inherit) { throw 'Unexpected materials ACE' }
}
$paths = [ordered]@{}
foreach ($name in @('capture', 'replay-hello', 'replay-query', 'hold-output')) {
    $path = Join-Path $materialRoot $name
    [IO.Directory]::CreateDirectory($path) | Out-Null
    $paths[$name] = $path
}
$receipt = [ordered]@{ status='materials_ready_no_authority'; root=$materialRoot; paths=$paths; acl_sddl=$actual.Sddl; raw_frames_are_sensitive_test_material=$true }
$json = $receipt | ConvertTo-Json -Depth 4
[IO.File]::WriteAllText((Join-Path $materialRoot 'acl-receipt.json'), $json + [Environment]::NewLine, [Text.UTF8Encoding]::new($false))
$json
