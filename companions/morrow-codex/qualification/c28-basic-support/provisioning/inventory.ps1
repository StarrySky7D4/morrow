
$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [System.Text.UTF8Encoding]::new($false)
$u = @(Get-LocalUser | Where-Object { $_.Name -in @('CodexSandboxOffline','CodexSandboxOnline') } | ForEach-Object { @{name=$_.Name;sid=$_.SID.Value} })
$g = @(Get-LocalGroup | Where-Object { $_.Name -eq 'CodexSandboxUsers' } | ForEach-Object { @{name=$_.Name;sid=$_.SID.Value} })
$r = (Test-Path -LiteralPath 'Registry::HKEY_LOCAL_MACHINE\SOFTWARE\OpenAI\Codex\WindowsSandboxService')
$s = @(Get-Service | Where-Object { $_.Name -like '*Codex*Sandbox*' } | ForEach-Object { $_.Name })
$f = @(Get-NetFirewallRule | Where-Object { $_.Name -in @('codex_sandbox_offline_block_outbound','codex_sandbox_offline_block_inbound','codex_sandbox_offline_block_loopback_tcp','codex_sandbox_offline_block_loopback_udp','codex_sandbox_offline_allow_loopback_proxy') } | ForEach-Object { $_.Name })
$c = Get-CimInstance -ClassName Win32_ComputerSystem
$p = [Security.Principal.WindowsPrincipal]::new([Security.Principal.WindowsIdentity]::GetCurrent())
@{manufacturer=[string]$c.Manufacturer;model=[string]$c.Model;hypervisor_present=[bool]$c.HypervisorPresent;administrator=$p.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator);users=$u;groups=$g;registration_exists=[bool]$r;services=$s;firewall_names=$f} | ConvertTo-Json -Depth 4 -Compress
