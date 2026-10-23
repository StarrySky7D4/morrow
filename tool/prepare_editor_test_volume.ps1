param([Parameter(Mandatory=$true)][string]$Output)
Set-StrictMode -Version Latest
$ErrorActionPreference='Stop'
$root=[IO.Path]::GetFullPath($Output).TrimEnd('\')
$allowed='D:\build_cache\_audit\'
if(-not $root.StartsWith($allowed,[StringComparison]::OrdinalIgnoreCase)-or (Test-Path -LiteralPath $root)){throw 'Use a fresh D-cache audit directory'}
$node=[IO.Path]::GetDirectoryName($root)
while($node){
 if(Test-Path -LiteralPath $node){if(([IO.File]::GetAttributes($node)-band 1024)-ne 0){throw 'No links in volume evidence path'}}
 $node=[IO.Path]::GetDirectoryName($node)
}
if((Get-Volume -DriveLetter D).SizeRemaining-lt 2GB){throw 'Outer D capacity reserve'}
New-Item -ItemType Directory -Path $root | Out-Null
$disk=Join-Path $root 'draft-test.vhdx'
$mount=Join-Path $root 'mount'
New-Item -ItemType Directory -Path $mount | Out-Null
@{scope='Fresh private 256 MiB editor persistence fixture';disk=$disk;mount=$mount;utc=[DateTime]::UtcNow.ToString('o')} | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $root 'create.reserved.json') -Encoding utf8
New-VHD -Path $disk -Dynamic -SizeBytes 256MB -BlockSizeBytes 1MB | Out-Null
$vhd=Mount-VHD -Path $disk -NoDriveLetter -PassThru
$actual=Get-VHD -Path $disk
if($actual.Path-cne $disk-or $actual.Size-ne 256MB-or -not $actual.Attached-or $actual.VhdType.ToString()-cne 'Dynamic'){throw 'Exact new VHD identity'}
$physical=Get-Disk -Number $vhd.DiskNumber
if($physical.Size-ne 256MB-or $physical.IsBoot-or $physical.IsSystem-or $physical.PartitionStyle.ToString()-cne 'RAW'-or $physical.BusType.ToString()-cnotin @('File Backed Virtual','FileBackedVirtual')){throw 'Only the new raw file-backed disk may be initialized'}
$initialized=Initialize-Disk -Number $physical.Number -PartitionStyle GPT -PassThru
$partition=New-Partition -DiskNumber $initialized.Number -UseMaximumSize
$volume=$partition | Format-Volume -FileSystem NTFS -NewFileSystemLabel 'MorrowDraftTest' -AllocationUnitSize 4096 -Confirm:$false
Add-PartitionAccessPath -DiskNumber $partition.DiskNumber -PartitionNumber $partition.PartitionNumber -AccessPath ($mount+'\')
$current=Get-Partition -DiskNumber $partition.DiskNumber -PartitionNumber $partition.PartitionNumber
$volume=$current | Get-Volume
if($volume.FileSystem-cne 'NTFS'-or $volume.FileSystemLabel-cne 'MorrowDraftTest'-or $volume.Size-ge 256MB-or $volume.Size-lt 128MB){throw 'Exact fresh private NTFS volume'}
$result=@{schema='MORROW_PRIVATE_TEST_VOLUME_V1';disk=$disk;mount=$mount;virtual_bytes=268435456;disk_unique_id=$physical.UniqueId;disk_number=$physical.Number;partition_number=$partition.PartitionNumber;volume_guid=$volume.Path;size=$volume.Size;drive_letter=$volume.DriveLetter;vm_registered=$false;host_volumes_formatted=$false;purpose='Real bounded disk-full and recovery; preserve VHDX after test'}
$result | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $root 'plan.json') -Encoding utf8
$result | ConvertTo-Json -Compress
