throw 'SOURCE_ONLY_DISABLED_STAGE19_ADAPTER003_INSTALLATION_PENDING_NO_VM_OR_LOAD'
# Descriptive definitions until a separately reviewed/authorized successor exists.
$InstallC28PumpDefinitions = {
    Set-StrictMode -Version Latest
    $ErrorActionPreference='Stop'
    if (Get-Variable -Name C28PumpContract -Scope Global -ErrorAction SilentlyContinue) { throw 'Installation definitions already exist; no replacement/adoption.' }
    $global:C28PumpContract=@{
        BindingState='PENDING_REAL_PRODUCTION_MANIFEST_GUEST_AND_ONCE_AUTHORITY'
        OperationId='7ba550b9-ad7f-499b-a56f-9711d9560856'
        TypeName='Morrow.C28.Autonomous.CapturePump'
        DllPath='C:\MorrowSdkQualification\C28-stage19-once001\bin\CapturePump.dll'
        DllBytes=15360L
        DllSHA256='bf5ae8c16ee79acb8de4c00510589af532376674048381fbc369228e793273fc'
        SourceSHA256='07c92459106ce8c3146cbf0d7ef8baaf9be7cc3c417b1f2b998d7bb4af6a1cdc'
        CompileReceiptSHA256='990da9b7ea2d7403aad6a2184168f79847d64e33f534c54b64d777c6b1a168c7'
        CompileRootSHA256='fdce0fb0a50629abb1a50a5ca5dbc952a97c7588e83eb88daad8830396c0fec7'
        PublicManifestSHA256='PENDING_REAL_THREE_PE_MANIFEST_SHA256'
        HarnessSHA256='PENDING_REAL_STAGE18_HARNESS_SHA256'
        HarnessBytes=0L
        GuestFirmwareUuid='PENDING_EXPLICIT_GUEST_IDENTITY'
        OnceAuthorizationSHA256='PENDING_SEPARATE_EXPLICIT_ONCE_AUTHORITY'
        ReuseOldAuthority=$false
    }
    function global:Assert-C28PumpBinding {
        $c=$global:C28PumpContract
        if ($c.BindingState -cne 'ROOT_REVIEWED_BOUND_AND_EXPLICIT_ONCE_AUTHORIZED' -or $c.ReuseOldAuthority -or $c.HarnessBytes -le 0 -or $c.HarnessBytes -gt 134217728) { throw 'Pump/production/once binding remains PENDING; no load.' }
        foreach($name in @('PublicManifestSHA256','HarnessSHA256','OnceAuthorizationSHA256')) {
            if ([string]$c[$name] -cnotmatch '^[0-9a-f]{64}$') { throw 'Missing real artifact or explicit authority pin.' }
        }
        $id=[Guid]::Empty
        if (-not [Guid]::TryParse([string]$c.GuestFirmwareUuid,[ref]$id) -or $id -eq [Guid]::Empty) { throw 'Missing explicit actual guest identity.' }
    }
    function global:Assert-C28PumpDllPath([string]$Path) {
        if ($Path -cne $global:C28PumpContract.DllPath -or $Path -notmatch '^C:\\' -or $Path.Substring(2).Contains(':') -or $Path -match '(?:^|\\)\.\.?($|\\)') { throw 'Fixed installation path required.' }
        $cursor=$Path
        while($cursor) {
            if (([IO.File]::GetAttributes($cursor) -band [IO.FileAttributes]::ReparsePoint) -ne 0) { throw 'Installation reparse rejected.' }
            $cursor=[IO.Path]::GetDirectoryName($cursor.TrimEnd('\'))
        }
    }
    function global:Install-C28ReviewedPump([string]$OperationId,[string]$Confirmation,[string]$OnceAuthorizationSHA256) {
        Assert-C28PumpBinding
        if ($OperationId -cne $global:C28PumpContract.OperationId -or $Confirmation -cne 'INSTALL_ONE_PINNED_CAPTURE_PUMP_FOR_7ba550b9-ad7f-499b-a56f-9711d9560856' -or $OnceAuthorizationSHA256 -cne $global:C28PumpContract.OnceAuthorizationSHA256) { throw 'Explicit installation identity/confirmation/authority binding required.' }
        if (Get-Variable -Name C28PumpInstallation -Scope Global -ErrorAction SilentlyContinue) { throw 'Installation already reserved; no second load/retry/adoption.' }
        $state=@{OperationId=$OperationId;State='RESERVED_UNKNOWN';PinStream=$null;Bytes=$null;Assembly=$null;PumpType=$null;LoadAttempted=$false;DllSHA256=$global:C28PumpContract.DllSHA256;PublicManifestSHA256=$global:C28PumpContract.PublicManifestSHA256;OnceAuthorizationSHA256=$OnceAuthorizationSHA256}
        $global:C28PumpInstallation=$state
        try {
            foreach($existing in [AppDomain]::CurrentDomain.GetAssemblies()) {
                if ($null -ne $existing.GetType($global:C28PumpContract.TypeName,$false,$false)) { throw 'Foreign/pre-existing pump type; no adoption or additional load.' }
            }
            Assert-C28PumpDllPath $global:C28PumpContract.DllPath
            $state.PinStream=[IO.File]::Open($global:C28PumpContract.DllPath,[IO.FileMode]::Open,[IO.FileAccess]::Read,[IO.FileShare]::Read)
            if ($state.PinStream.Length -ne 15360L) { throw 'Reviewed DLL size differs.' }
            $raw=New-Object byte[] 15360
            $offset=0
            while($offset -lt $raw.Length) {
                $read=$state.PinStream.Read($raw,$offset,$raw.Length-$offset)
                if ($read -eq 0) { throw 'Reviewed DLL shortened.' }
                $offset+=$read
            }
            $hash=[Security.Cryptography.SHA256]::Create()
            try { $sha=[BitConverter]::ToString($hash.ComputeHash($raw)).Replace('-','').ToLowerInvariant() } finally { $hash.Dispose() }
            if ($sha -cne 'bf5ae8c16ee79acb8de4c00510589af532376674048381fbc369228e793273fc') { throw 'Reviewed DLL bytes differ.' }
            $state.Bytes=$raw
            $state.LoadAttempted=$true # Before the only actual loader call; failure is never replayed.
            $state.Assembly=[Reflection.Assembly]::Load([byte[]]$state.Bytes)
            $state.PumpType=$state.Assembly.GetType('Morrow.C28.Autonomous.CapturePump',$true,$false)
            if (-not [Object]::ReferenceEquals($state.PumpType.Assembly,$state.Assembly) -or -not $state.PumpType.IsPublic -or $state.PumpType.IsAbstract -or $null -eq $state.PumpType.GetConstructor([type[]]@([string]))) { throw 'Loaded pump type/assembly contract differs.' }
            Assert-C28PumpDllPath $global:C28PumpContract.DllPath
            if ($state.PinStream.Length -ne 15360L) { throw 'Held installation image changed.' }
            $state.State='LOADED_SAME_REVIEWED_ASSEMBLY'
            [pscustomobject]@{Kind='PUMP_INSTALLED';OperationId=$OperationId;DllSHA256=$state.DllSHA256;TypeName=$state.PumpType.FullName;Scope='ASSEMBLY_IDENTITY_ONLY_NOT_NATIVE_OR_SDK_ACCEPTANCE'}
        } catch {
            $state.State='UNKNOWN_RETAINED_NO_REPLAY'
            # Keep any acquired stream, loaded Assembly/type and original bytes reachable.
            # No Dispose/reload/unload/credential/child/cleanup occurs on this path.
            throw 'Pump installation Unknown; original reservation/resources retained.'
        }
    }
    function global:Assert-C28InstalledPump([string]$OperationId,[string]$PublicManifestSHA256,[string]$OnceAuthorizationSHA256) {
        Assert-C28PumpBinding
        if (-not (Get-Variable -Name C28PumpInstallation -Scope Global -ErrorAction SilentlyContinue)) { throw 'Explicit pump installation missing.' }
        $s=$global:C28PumpInstallation
        if ($OperationId -cne $global:C28PumpContract.OperationId -or $s.OperationId -cne $OperationId -or $s.State -cne 'LOADED_SAME_REVIEWED_ASSEMBLY' -or -not $s.LoadAttempted -or $s.DllSHA256 -cne 'bf5ae8c16ee79acb8de4c00510589af532376674048381fbc369228e793273fc' -or $PublicManifestSHA256 -cne $global:C28PumpContract.PublicManifestSHA256 -or $s.PublicManifestSHA256 -cne $PublicManifestSHA256 -or $OnceAuthorizationSHA256 -cne $global:C28PumpContract.OnceAuthorizationSHA256 -or $s.OnceAuthorizationSHA256 -cne $OnceAuthorizationSHA256) { throw 'Installed pump or operation/manifest/authority identity differs.' }
        if ($null -eq $s.PinStream -or -not $s.PinStream.CanRead -or $s.PinStream.Length -ne 15360L -or $null -eq $s.Assembly -or $null -eq $s.PumpType -or -not [Object]::ReferenceEquals($s.PumpType.Assembly,$s.Assembly) -or $s.PumpType.FullName -cne 'Morrow.C28.Autonomous.CapturePump') { throw 'Held assembly/type/file identity lost.' }
        return $s
    }
}
