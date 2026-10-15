
$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [System.Text.UTF8Encoding]::new($false)
$u = @(Get-LocalUser | Where-Object { $_.Name -in @('CodexSandboxOffline','CodexSandboxOnline') } | ForEach-Object { @{name=$_.Name;sid=$_.SID.Value} })
$g = @(Get-LocalGroup | Where-Object { $_.Name -eq 'CodexSandboxUsers' } | ForEach-Object { @{name=$_.Name;sid=$_.SID.Value} })
$r = (Test-Path -LiteralPath 'Registry::HKEY_LOCAL_MACHINE\SOFTWARE\OpenAI\Codex\WindowsSandboxService')
$s = @(Get-Service | Where-Object { $_.Name -like '*Codex*Sandbox*' } | ForEach-Object { $_.Name })
# Match the same INetFwRule.Name identity used by the opaque setup proof and
# INetFwRules.Item. NetSecurity.Name is a GUID, not this COM rule name.
$fixedRuleNames = @('codex_sandbox_offline_block_outbound','codex_sandbox_offline_block_inbound','codex_sandbox_offline_block_loopback_tcp','codex_sandbox_offline_block_loopback_udp','codex_sandbox_offline_allow_loopback_proxy')
$f = @()
$firewallPolicy = $null
$firewallRules = $null
try {
    $firewallPolicy = New-Object -ComObject HNetCfg.FwPolicy2
    $firewallRules = $firewallPolicy.Rules
    foreach ($rule in $firewallRules) {
        try {
            $name = [string]$rule.Name
            if (($fixedRuleNames -contains $name) -and -not ($fixedRuleNames -ccontains $name)) {
                throw 'noncanonical fixed firewall rule name; STOP without adoption'
            }
            if ($fixedRuleNames -ccontains $name) {
                # Retain every exact match, including duplicates: never deduplicate
                # a collision into an apparently valid owned inventory.
                $f += $name
                if ($f.Count -gt 5) { throw 'inventory firewall resource bounds' }
            }
        } finally {
            if ($null -ne $rule -and [Runtime.InteropServices.Marshal]::IsComObject($rule)) {
                [void][Runtime.InteropServices.Marshal]::ReleaseComObject($rule)
            }
        }
    }
} finally {
    if ($null -ne $firewallRules -and [Runtime.InteropServices.Marshal]::IsComObject($firewallRules)) {
        [void][Runtime.InteropServices.Marshal]::ReleaseComObject($firewallRules)
    }
    if ($null -ne $firewallPolicy -and [Runtime.InteropServices.Marshal]::IsComObject($firewallPolicy)) {
        [void][Runtime.InteropServices.Marshal]::ReleaseComObject($firewallPolicy)
    }
}
$c = Get-CimInstance -ClassName Win32_ComputerSystem
$p = [Security.Principal.WindowsPrincipal]::new([Security.Principal.WindowsIdentity]::GetCurrent())
@{manufacturer=[string]$c.Manufacturer;model=[string]$c.Model;hypervisor_present=[bool]$c.HypervisorPresent;administrator=$p.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator);users=$u;groups=$g;registration_exists=[bool]$r;services=$s;firewall_names=$f} | ConvertTo-Json -Depth 4 -Compress
