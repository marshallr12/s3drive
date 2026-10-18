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
    $child=[Diagnostics.Process]::Start($info)
    try {
        $output=$child.StandardOutput.ReadToEndAsync()
        $errors=$child.StandardError.ReadToEndAsync()
        if (-not $child.WaitForExit($(if($Mode -eq 'Recover'){120000}else{30000}))) { $child.Kill(); throw 'Coordinator Rust client timed out' }
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
        'AuthorityDiagnostics' {
            if ([IO.File]::Exists((Join-Path $state 'startup.json'))) { try { Manage Status } catch { Write-Output $_.Exception.Message } }
            if ([IO.File]::Exists((Join-Path $state 'session.log'))) { Get-Content -LiteralPath (Join-Path $state 'session.log') -Tail 30 }
        }
        default { throw 'Unknown coordinator fixture action' }
    }
} catch { [Console]::Error.WriteLine($_.Exception.Message); exit 1 }
exit 0
