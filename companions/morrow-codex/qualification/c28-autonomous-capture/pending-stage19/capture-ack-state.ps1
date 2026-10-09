# Definition-only shared implementation. No process, file, network or VM effect.
$CaptureAckDefinitions = {
    function global:New-C28AckStream([string]$Name,[long]$Limit,[int]$Capacity) {
        if ($Name -cnotin @('Out','Err','journal') -or $Limit -le 0 -or $Capacity -le 0) { throw 'Fixed capture stream required.' }
        return @{Name=$Name;Limit=$Limit;Capacity=$Capacity;Observed=0L;Emitted=0L;SavedAck=0L;Retained=0L;Frames=@();EOF=$false;DrainOnly=$false;Discarded=0L;Unknown=$false}
    }
    function global:Set-C28AckDrain($State) {
        $State.Unknown=$true;$State.DrainOnly=$true
        $State.Discarded+=$State.Retained;$State.Retained=0L;$State.Frames=@()
    }
    function global:Confirm-C28SavedAck($State,[long]$Ack,[bool]$DrainOnly) {
        if ($Ack -lt $State.SavedAck -or $Ack -gt $State.Emitted) { Set-C28AckDrain $State;throw 'Capture saved ack outside emitted range.' }
        if ($Ack -ne $State.SavedAck) {
            $boundary=$false
            foreach($frame in $State.Frames) { if ($frame.Offset+$frame.Bytes.Length -eq $Ack) { $boundary=$true } }
            if (-not $boundary) { Set-C28AckDrain $State;throw 'Capture saved ack must end a retained frame.' }
            $remaining=@()
            foreach($frame in $State.Frames) {
                if ($frame.Offset+$frame.Bytes.Length -le $Ack) { $State.Retained-=$frame.Bytes.Length }
                else { $remaining+=,$frame }
            }
            $State.Frames=$remaining;$State.SavedAck=$Ack
        }
        if ($DrainOnly) { Set-C28AckDrain $State }
    }
    function global:Add-C28ObservedBytes($State,[byte[]]$Bytes) {
        if ($State.EOF -or $Bytes.Length -le 0 -or $Bytes.Length -gt 16384) { Set-C28AckDrain $State;throw 'Invalid capture read after EOF or frame bound.' }
        $offset=$State.Observed
        if ($offset -gt [long]::MaxValue-$Bytes.Length) { Set-C28AckDrain $State;throw 'Capture count overflow.' }
        $State.Observed+=$Bytes.Length
        if ($State.Observed -gt $State.Limit -or $State.Retained+$Bytes.Length -gt $State.Capacity) { Set-C28AckDrain $State }
        if ($State.DrainOnly) { $State.Discarded+=$Bytes.Length;return }
        $copy=New-Object byte[] $Bytes.Length;[Array]::Copy($Bytes,$copy,$Bytes.Length)
        $State.Frames+=,@{Offset=[long]$offset;Bytes=$copy};$State.Retained+=$copy.Length
    }
    function global:Get-C28AckFrames($State,[int]$GuestPid) {
        foreach($frame in $State.Frames) {
            $end=$frame.Offset+$frame.Bytes.Length
            if ($end -gt $State.Emitted) { $State.Emitted=$end }
            [pscustomobject]@{Kind='CAPTURE_BYTES';Pid=$GuestPid;Stream=$State.Name;Offset=$frame.Offset;Bytes=[Convert]::ToBase64String($frame.Bytes)}
        }
    }
    function global:New-C28HostStream([string]$Name) {
        if ($Name -cnotin @('Out','Err','journal')) { throw 'Fixed host stream required.' }
        return @{Name=$Name;Received=0L;Saved=0L;EOF=$false;DrainOnly=$false;Unknown=$false;Discarded=0L}
    }
    function global:Write-C28HostFrame($State,$Sink,[long]$Offset,[byte[]]$Bytes,[long]$Limit) {
        if ($Offset -ne $State.Received -or $Bytes.Length -le 0 -or $Bytes.Length -gt 16384 -or $Offset -gt $Limit-$Bytes.Length -or $State.EOF) {
            $State.Unknown=$true;$State.DrainOnly=$true;throw 'Host capture range rejected.'
        }
        $State.Received+=$Bytes.Length
        if ($State.DrainOnly) { $State.Discarded+=$Bytes.Length;return }
        try {
            # Neither Saved nor a subsequent saved ack advances on partial Write/Flush failure.
            $Sink.Write($Bytes,0,$Bytes.Length);$Sink.Flush($true);$State.Saved+=$Bytes.Length
        } catch { $State.Unknown=$true;$State.DrainOnly=$true;$State.Discarded+=$Bytes.Length }
    }
    function global:Observe-C28Terminal($State,[int]$GuestPid,[int]$Exit) {
        if ($null -eq $State.Pid) { $State.Pid=$GuestPid;$State.Exit=$Exit;return }
        if ($State.Pid -ne $GuestPid -or $State.Exit -ne $Exit) { $State.Unknown=$true;throw 'Frozen actual terminal changed.' }
    }
    function global:Assert-C28CaptureClosed($Guest,$HostState) {
        if ($Guest.Unknown -or $HostState.Unknown -or $Guest.DrainOnly -or $HostState.DrainOnly -or -not $Guest.EOF -or -not $HostState.EOF -or $Guest.Observed -ne $Guest.SavedAck -or $Guest.SavedAck -ne $HostState.Saved -or $HostState.Saved -ne $HostState.Received -or $Guest.Retained -ne 0) { throw 'Capture cannot be accepted.' }
    }
}
