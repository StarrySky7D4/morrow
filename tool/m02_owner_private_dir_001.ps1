param([Parameter(Mandatory=$true)][string]$Path)
$ErrorActionPreference='Stop'
$item=Get-Item -LiteralPath $Path
if (-not $item.PSIsContainer -or @(Get-ChildItem -LiteralPath $Path -Force).Count -ne 0) { throw 'Only new empty test directory' }
$parent=$item
while ($null -ne $parent) { if (($parent.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) { throw 'Reparse path rejected' }; $parent=$parent.Parent }
$sid=[Security.Principal.WindowsIdentity]::GetCurrent().User
$acl=[Security.AccessControl.DirectorySecurity]::new()
$acl.SetOwner($sid); $acl.SetAccessRuleProtection($true,$false)
foreach($who in @($sid,[Security.Principal.SecurityIdentifier]::new('S-1-5-18'))) { $acl.AddAccessRule([Security.AccessControl.FileSystemAccessRule]::new($who,'FullControl','ContainerInherit,ObjectInherit','None','Allow')) }
Set-Acl -LiteralPath $Path -AclObject $acl
$actual=Get-Acl -LiteralPath $Path
if(-not $actual.AreAccessRulesProtected){throw 'ACL not protected'}
$actual.Sddl
