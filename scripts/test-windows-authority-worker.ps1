#requires -Version 5.1
# Called only inside test-windows-session.ps1's newly created ordinary account.
param([Parameter(Mandatory)][string]$Operation)
$ErrorActionPreference='Stop'
Set-StrictMode -Version Latest
$root=$PSScriptRoot
$exe=Join-Path $root 's3drive.exe'
$manager=Join-Path $root 'source-manager.ps1'
$state=Join-Path $root 'authority-startup'
$db=Join-Path $root 'authority-state'
$inputs=Join-Path $root 'authority-inputs'
$identityFile=Join-Path $root 'authority-identity.json'
$driveState=Join-Path $root 'managed-startup'
$driveMount=Join-Path $root 'managed-mounted'
$directoryName=(-join @([char]0x8CC7,[char]0x6599,' caf',[char]0xE9,' ',[char]::ConvertFromUtf32(0x1F980)))
$fileName=(-join @([char]0x65E5,[char]0x672C,[char]0x8A9E,' %2F + caf',[char]0xE9,'.bin'))
$binaryPath=Join-Path $driveMount ($directoryName+'\'+$fileName)
$emptyPath=Join-Path $driveMount ('empty '+[char]::ConvertFromUtf32(0x1F980))
function Quote([string]$Text) {
    if ($Text.Contains('"') -or $Text.Contains([char]0)) { throw 'Invalid fixture argument' }
    return '"'+[regex]::Replace($Text,'(\\+)$','$1$1')+'"'
}
function Check-Worker([string]$Mode) {
    $info=[Diagnostics.ProcessStartInfo]::new()
    $info.FileName=Join-Path $root 'authority-worker.exe'
    $info.Arguments='--ignored --exact windows_authority_fixture_worker --nocapture'
    $info.UseShellExecute=$false
    $info.CreateNoWindow=$true
    $info.RedirectStandardOutput=$true
    $info.RedirectStandardError=$true
    $info.EnvironmentVariables['S3DRIVE_AUTHORITY_FIXTURE_ROOT']=$root
    $info.EnvironmentVariables['S3DRIVE_AUTHORITY_FIXTURE_MODE']=$Mode
    if ([IO.File]::Exists((Join-Path $root 'managed-fixture.json'))) {
        $profile=[IO.File]::ReadAllText((Join-Path $root 'profile-path.txt'))
        $info.EnvironmentVariables['AWS_SHARED_CREDENTIALS_FILE']=Join-Path $profile '.aws\credentials'
        $info.EnvironmentVariables['AWS_CONFIG_FILE']=Join-Path $profile '.aws\config'
        $info.EnvironmentVariables['AWS_EC2_METADATA_DISABLED']='true'
        $info.EnvironmentVariables['S3DRIVE_NATIVE_MANAGED_EXECUTABLE']=$exe
    }
    $child=[Diagnostics.Process]::Start($info)
    try {
        $output=$child.StandardOutput.ReadToEndAsync()
        $errors=$child.StandardError.ReadToEndAsync()
        if (-not $child.WaitForExit($(if($Mode -eq 'Recover'){120000}elseif($Mode -eq 'Seed'){90000}else{30000}))) { $child.Kill(); throw 'Coordinator Rust client timed out' }
        $text=$output.GetAwaiter().GetResult()
        $errorText=$errors.GetAwaiter().GetResult()
        if ($child.ExitCode -ne 0) { throw "Rust coordinator fixture failed: $text $errorText" }
    } finally { $child.Dispose() }
}
function Manage([string]$Action) {
    $text=& $manager -Action $Action -Component Authority -Name acceptance -StateDirectory $state
    if ($LASTEXITCODE -ne 0) { throw "Coordinator manager $Action failed" }
    return $text
}
function Manage-Drive([string]$Action) {
    $text=& $manager -Action $Action -Name managed -StateDirectory $driveState
    if ($LASTEXITCODE -ne 0) { throw "Managed drive $Action failed" }
    return $text
}
function Fixture-Control([string]$Action) {
    $id=[Guid]::NewGuid().ToString('N')
    $path=Join-Path $root 'control-request.json'
    $temporary=Join-Path $root 'control-request.tmp'
    [IO.File]::WriteAllText($temporary,(@{id=$id;action=$Action}|ConvertTo-Json -Compress),[Text.UTF8Encoding]::new($false))
    if ([IO.File]::Exists($path)) { [IO.File]::Delete($path) }
    [IO.File]::Move($temporary,$path)
    $deadline=[DateTime]::UtcNow.AddSeconds(15)
    while ([DateTime]::UtcNow -lt $deadline) {
        try {
            $reply=Get-Content -LiteralPath (Join-Path $root 'control-reply.json') -Raw -Encoding UTF8 | ConvertFrom-Json
            if ($reply.id -eq $id) { return $reply }
        } catch {}
        Start-Sleep -Milliseconds 50
    }
    throw 'Owned S3 fixture control timed out'
}
function Read-StreamBytes([IO.FileStream]$Stream) {
    $Stream.Position=0
    $bytes=[byte[]]::new($Stream.Length)
    $offset=0
    while ($offset -lt $bytes.Length) {
        $read=$Stream.Read($bytes,$offset,$bytes.Length-$offset)
        if ($read -eq 0) { throw 'Short native file read' }
        $offset+=$read
    }
    return ,$bytes
}
function Current-Runtime {
    return (Get-Content -LiteralPath (Join-Path $state 'runtime.json') -Raw -Encoding UTF8 | ConvertFrom-Json)
}
function Assert-Settings {
    $scheduler=New-Object -ComObject Schedule.Service
    $scheduler.Connect()
    $task=$scheduler.GetFolder('\').GetTask("S3DriveAuthority-$([Security.Principal.WindowsIdentity]::GetCurrent().User.Value)-acceptance")
    $d=$task.Definition
    if ($d.Principal.LogonType -ne 1 -or $d.Principal.RunLevel -ne 0 -or
        $d.Settings.ExecutionTimeLimit -ne 'PT0S' -or $d.Settings.MultipleInstances -ne 2 -or
        $d.Settings.AllowHardTerminate -or $d.Settings.RestartCount -ne 3 -or
        $d.Settings.RestartInterval -ne 'PT1M' -or $d.Triggers.Count -ne 1 -or
        $d.Triggers.Item(1).Type -ne 9) { throw 'Coordinator task lifetime settings changed' }
}
try {
    switch ($Operation) {
        'AuthorityInstall' {
            $reader=[IO.StreamReader]::new([Console]::OpenStandardInput(),[Text.Encoding]::UTF8,$true)
            try { $received=$reader.ReadLine() } finally { $reader.Dispose() }
            if (-not $received.StartsWith('S3d!aA9-') -or $received.Length -ne 40) { throw 'Invalid fixture credential pipe' }
            $secure=ConvertTo-SecureString $received -AsPlainText -Force
            $received=$null
            $credential=[Management.Automation.PSCredential]::new([Security.Principal.WindowsIdentity]::GetCurrent().Name,$secure)
            Check-Worker Init
            $info=[Diagnostics.ProcessStartInfo]::new()
            $info.FileName=$exe
            $fixtureArgs=@('authority-serve','--directory',$db,'--token-file',(Join-Path $inputs 'authority.token'),
                '--listen','127.0.0.1:0','--name-policy','portable-collision-v1',
                '--tls-cert',(Join-Path $inputs 'cert.pem'),'--tls-key',(Join-Path $inputs 'key.pem'),'--stop-on-stdin-close')
            $info.Arguments=($fixtureArgs | ForEach-Object { Quote $_ }) -join ' '
            $info.UseShellExecute=$false
            $info.CreateNoWindow=$true
            $info.RedirectStandardInput=$true
            $info.RedirectStandardOutput=$true
            $info.RedirectStandardError=$true
            $child=[Diagnostics.Process]::Start($info)
            try {
                $errors=$child.StandardError.ReadToEndAsync()
                $line=$child.StandardOutput.ReadLineAsync()
                if (-not $line.Wait(30000)) { throw 'Bootstrap readiness timeout' }
                $ready=$line.GetAwaiter().GetResult()
                if (-not $ready) { throw ('Bootstrap failed: '+$errors.GetAwaiter().GetResult()) }
                $value=$ready | ConvertFrom-Json
                if ($value.endpoint -notmatch '^https://127\.0\.0\.1:[1-9][0-9]*/v1/namespace$') { throw 'Invalid TLS endpoint' }
                [IO.File]::WriteAllText($identityFile,$ready,[Text.UTF8Encoding]::new($false))
                $child.StandardInput.Close()
                if (-not $child.WaitForExit(30000) -or $child.ExitCode -ne 0) { throw 'Bootstrap did not stop cleanly' }
            } finally {
                if (-not $child.HasExited) { $child.StandardInput.Close(); if (-not $child.WaitForExit(10000)) { $child.Kill(); $child.WaitForExit() } }
                $child.Dispose()
            }
            # Offline preflight refuses a different identity before registration.
            $badState=Join-Path $root 'authority-invalid'
            $common=@{Action='Install';Component='Authority';Name='acceptance';StateDirectory=$badState;
                Executable=$exe;AuthorityDirectory=$db;AuthorityVolume=('f'*32);AuthorityNamePolicy='portable-collision-v1';
                AuthorityTokenFile=(Join-Path $inputs 'authority.token');AuthorityListen=([uri]$value.endpoint).Authority;
                TlsCert=(Join-Path $inputs 'cert.pem');TlsKey=(Join-Path $inputs 'key.pem');Credential=$credential}
            $null=& $manager @common 2>&1
            if ($LASTEXITCODE -eq 0 -or [IO.Directory]::Exists($badState)) { throw 'Wrong volume was registered or made startup state' }
            $common.StateDirectory=$state
            $common.AuthorityVolume=$value.volume
            & $manager @common
            if ($LASTEXITCODE -ne 0) { throw 'Coordinator registration failed' }
            Assert-Settings
            $null=Manage Start
            Check-Worker Seed
            # Original broad fixture copies are no longer needed; only the
            # account-private inputs are used by service and trusted Rust client.
            Remove-Item -LiteralPath (Join-Path $root 'key.pem') -Force
        }
        'AuthorityProbe' {
            Check-Worker Verify
            $before=Current-Runtime
            1..3 | ForEach-Object { $null=Manage Start }
            Check-Worker Verify
            $after=Current-Runtime
            if ($before.child_pid -ne $after.child_pid -or $before.supervisor_pid -ne $after.supervisor_pid) { throw 'Duplicate coordinator start replaced owner' }
            $logon=(& (Join-Path $env:SystemRoot 'System32\whoami.exe') /logonid | Out-String).Trim()
            if ($before.owner_sid -ne [Security.Principal.WindowsIdentity]::GetCurrent().User.Value -or
                $before.logon -eq $logon -or $before.logon -notmatch 'S-1-5-5-' -or $logon -notmatch 'S-1-5-5-') { throw 'Independent coordinator client logon not established' }
            $status=Manage Status | Out-String | ConvertFrom-Json
            $pin=Get-Content -LiteralPath $identityFile -Raw | ConvertFrom-Json
            if (-not $status.supervisor_running -or $status.component -ne 'authority' -or
                $status.expected_volume -ne $pin.volume -or $status.provider_access_verified) { throw 'Incorrect coordinator status' }
            @{child_pid=$before.child_pid;supervisor_pid=$before.supervisor_pid;owner_sid=$before.owner_sid;caller_logon=$logon;supervisor_logon=$before.logon} | ConvertTo-Json -Compress
        }
        'AuthorityCrash' {
            Check-Worker Verify
            $runtime=Current-Runtime
            # A runtime PID is only a hint. Open a handle, then check its image,
            # exact pinned command line and parent before killing our fixture.
            $child=[Diagnostics.Process]::GetProcessById($runtime.child_pid)
            try {
                $null=$child.Handle
                $native=Get-CimInstance Win32_Process -Filter "ProcessId=$($child.Id)"
                $pin=Get-Content -LiteralPath $identityFile -Raw | ConvertFrom-Json
                if ($child.MainModule.FileName -ine $exe -or $native.ParentProcessId -ne $runtime.supervisor_pid -or
                    -not $native.CommandLine.Contains('authority-serve') -or
                    -not $native.CommandLine.Contains('--expect-volume') -or -not $native.CommandLine.Contains($pin.volume)) { throw 'Fixture coordinator identity changed; refusing kill' }
                $runtime | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $root 'authority-before-crash.json') -Encoding UTF8
                $child.Kill()
                $child.WaitForExit()
            } finally { $child.Dispose() }
            # No Start here: recovery must come from Task Scheduler's retry.
        }
        'AuthorityRecover' {
            Check-Worker Recover
            $before=Get-Content -LiteralPath (Join-Path $root 'authority-before-crash.json') -Raw | ConvertFrom-Json
            $after=Current-Runtime
            if ($before.child_pid -eq $after.child_pid -or $before.supervisor_pid -eq $after.supervisor_pid) { throw 'Coordinator did not restart with a fresh supervised process' }
            @{child_pid=$after.child_pid;supervisor_pid=$after.supervisor_pid;owner_sid=$after.owner_sid;recovered=$true} | ConvertTo-Json -Compress
        }
        'AuthorityRemove' {
            Check-Worker Verify
            $null=Manage Remove
            foreach ($path in @((Join-Path $db 'metadata.sqlite3'),(Join-Path $inputs 'authority.token'),(Join-Path $inputs 'cert.pem'),(Join-Path $inputs 'key.pem'),$identityFile)) {
                if (-not [IO.File]::Exists($path)) { throw 'Removal deleted coordinator metadata or private inputs' }
            }
            $pin=Get-Content -LiteralPath $identityFile -Raw | ConvertFrom-Json
            & $exe authority-check --directory $db --expect-volume $pin.volume --name-policy portable-collision-v1 --token-file (Join-Path $inputs 'authority.token') --listen ([uri]$pin.endpoint).Authority --tls-cert (Join-Path $inputs 'cert.pem') --tls-key (Join-Path $inputs 'key.pem')
            if ($LASTEXITCODE -ne 0) { throw 'Removed coordinator database no longer matches original identity' }
        }
        'ManagedInstall' {
            $reader=[IO.StreamReader]::new([Console]::OpenStandardInput(),[Text.Encoding]::UTF8,$true)
            try { $received=$reader.ReadLine() } finally { $reader.Dispose() }
            if (-not $received.StartsWith('S3d!aA9-') -or $received.Length -ne 40) { throw 'Invalid fixture credential pipe' }
            $secure=ConvertTo-SecureString $received -AsPlainText -Force
            $received=$null
            $credential=[Management.Automation.PSCredential]::new([Security.Principal.WindowsIdentity]::GetCurrent().Name,$secure)
            & $manager -Action Install -Name managed -StateDirectory $driveState -Executable $exe -Config (Join-Path $root 'managed.toml') -MountPoint $driveMount -Credential $credential
            if ($LASTEXITCODE -ne 0) { throw 'Managed startup registration failed' }
            $null=Manage-Drive Start
        }
        'ManagedProbe' {
            Check-Worker Verify
            $deadline=[DateTime]::UtcNow.AddSeconds(45)
            while ($true) {
                try { $bytes=[IO.File]::ReadAllBytes($binaryPath); break }
                catch { if ([DateTime]::UtcNow -gt $deadline) { throw }; Start-Sleep -Milliseconds 100 }
            }
            if ($bytes.Length -ne 70123) { throw 'Wrong managed binary length' }
            for ($i=0;$i -lt $bytes.Length;$i++) { if ($bytes[$i] -ne ($i % 251)) { throw 'Wrong binary data across Windows logons' } }
            if ([IO.File]::ReadAllBytes($emptyPath).Length -ne 0 -or
                [IO.File]::ReadAllText((Join-Path $driveMount 'other bucket.txt')) -ne 'payload from the second bucket') { throw 'Wrong empty or second-store data' }
            $before=Get-Content -LiteralPath (Join-Path $driveState 'runtime.json') -Raw -Encoding UTF8 | ConvertFrom-Json
            1..3 | ForEach-Object { $null=Manage-Drive Start }
            $after=Get-Content -LiteralPath (Join-Path $driveState 'runtime.json') -Raw -Encoding UTF8 | ConvertFrom-Json
            if ($before.child_pid -ne $after.child_pid -or $before.supervisor_pid -ne $after.supervisor_pid) { throw 'Duplicate managed start replaced owner' }
            $logon=(& (Join-Path $env:SystemRoot 'System32\whoami.exe') /logonid | Out-String).Trim()
            if ($before.logon -eq $logon -or $before.owner_sid -ne [Security.Principal.WindowsIdentity]::GetCurrent().User.Value) { throw 'Managed client not in independent owner logon' }
            @{child_pid=$before.child_pid;owner_sid=$before.owner_sid;caller_logon=$logon;supervisor_logon=$before.logon}|ConvertTo-Json -Compress
        }
        'ManagedExercise' {
            $stream=[IO.FileStream]::new($binaryPath,[IO.FileMode]::Open,[IO.FileAccess]::ReadWrite,([IO.FileShare]::ReadWrite -bor [IO.FileShare]::Delete),1,[IO.FileOptions]::None)
            try {
                $baseline=(Fixture-Control hide).puts
                [byte[]]$expected=[Text.Encoding]::UTF8.GetBytes('paired Windows binary')+[byte[]]@(0,255)
                $stream.SetLength(0)
                $stream.Write($expected,0,$expected.Length)
                $failed=$false
                try { $stream.Flush($true) } catch { $failed=$true }
                if (-not $failed -or (Fixture-Control count).puts -ne ($baseline+1)) { throw 'Lost upload response did not retain exactly one uncertain PUT' }
                Check-Worker Snapshot
                & $PSCommandPath -Operation AuthorityCrash
                if ($LASTEXITCODE -ne 0) { throw 'Owned coordinator crash failed' }
                $failed=$false
                try { $stream.Flush($true) } catch { $failed=$true }
                if (-not $failed -or [Convert]::ToBase64String((Read-StreamBytes $stream)) -ne [Convert]::ToBase64String($expected)) { throw 'Offline flush lost native dirty bytes' }
                $null=Fixture-Control restore
                & $PSCommandPath -Operation AuthorityRecover
                if ($LASTEXITCODE -ne 0) { throw 'Scheduled coordinator recovery failed' }
                $stream.Flush($true)
                Check-Worker CheckRecovered
                if ((Fixture-Control count).puts -ne ($baseline+1)) { throw 'Coordinator recovery repeated immutable PUT' }
                $stream.Position=0
                $dirty=[Text.Encoding]::UTF8.GetBytes('dirty conflict')
                $stream.Write($dirty,0,$dirty.Length)
                $local=Read-StreamBytes $stream
                Check-Worker Rival
                $failed=$false
                try { $stream.Flush($true) } catch { $failed=$true }
                if (-not $failed -or [Convert]::ToBase64String((Read-StreamBytes $stream)) -ne [Convert]::ToBase64String($local)) { throw 'Rival publication discarded local edits' }
                if ((Fixture-Control count).puts -ne ($baseline+2)) { throw 'Conflicted generation silently published' }
            } finally { $stream.Dispose() }
        }
        'ManagedRemove' {
            $null=Manage-Drive Remove
            $scheduler=New-Object -ComObject Schedule.Service
            $scheduler.Connect()
            $removed=$false
            try { $null=$scheduler.GetFolder('\').GetTask("S3Drive-$([Security.Principal.WindowsIdentity]::GetCurrent().User.Value)-managed") }
            catch { if ($_.Exception.GetBaseException().HResult -eq -2147024894) { $removed=$true } else { throw } }
            if (-not $removed) { throw 'Managed task remains after removal' }
            if ([IO.Directory]::Exists($driveMount)) { throw 'Managed drive remains after removal' }
            Check-Worker Verify
            Check-Worker CheckConflict
            if (-not [IO.File]::Exists((Join-Path $root 'managed-write-state\metadata.sqlite3'))) { throw 'Removal deleted dirty managed journal' }
        }
        'AuthorityDiagnostics' {
            if ([IO.File]::Exists((Join-Path $driveState 'session.log'))) { Get-Content -LiteralPath (Join-Path $driveState 'session.log') -Tail 30 }
            if ([IO.File]::Exists((Join-Path $state 'startup.json'))) { try { Manage Status } catch { Write-Output $_.Exception.Message } }
            if ([IO.File]::Exists((Join-Path $state 'session.log'))) { Get-Content -LiteralPath (Join-Path $state 'session.log') -Tail 30 }
        }
        default { throw 'Unknown coordinator fixture action' }
    }
} catch { [Console]::Error.WriteLine($_.Exception.Message); exit 1 }
exit 0
