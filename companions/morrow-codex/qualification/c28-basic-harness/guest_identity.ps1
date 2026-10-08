$ErrorActionPreference = 'Stop'
$system = Get-CimInstance -ClassName Win32_ComputerSystem
$product = Get-CimInstance -ClassName Win32_ComputerSystemProduct
$principal = [Security.Principal.WindowsPrincipal]::new([Security.Principal.WindowsIdentity]::GetCurrent())
[pscustomobject]@{
    schema = 'morrow-dedicated-hyperv-guest-identity-v1'
    manufacturer = [string]$system.Manufacturer
    model = [string]$system.Model
    firmware_uuid = ([string]$product.UUID).ToLowerInvariant()
    machine_name = [Environment]::MachineName
    hypervisor_present = [bool]$system.HypervisorPresent
    administrator = $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
    process_64_bit = [Environment]::Is64BitProcess
    os_64_bit = [Environment]::Is64BitOperatingSystem
    observed_utc = [DateTime]::UtcNow.ToString('o')
} | ConvertTo-Json -Compress
