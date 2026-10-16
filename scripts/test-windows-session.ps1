#requires -Version 5.1
# Explicit disposable-runner acceptance. Never run this against an existing
# user account: it creates and removes its own non-administrator local user.
param([Parameter(Mandatory)][string]$Executable, [Parameter(Mandatory)][string]$Config)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$runner = [Security.Principal.WindowsIdentity]::GetCurrent()
if (-not ([Security.Principal.WindowsPrincipal]::new($runner)).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
    throw 'Native session acceptance requires an elevated disposable Windows runner'
}
$suffix = [Guid]::NewGuid().ToString('N').Substring(0,10)
$userName = "s3d$suffix"
$plainPassword = 'S3d!aA9-' + [Guid]::NewGuid().ToString('N')
$password = ConvertTo-SecureString $plainPassword -AsPlainText -Force
$root = Join-Path ([IO.Path]::GetTempPath()) ("s3drive session caf" + [char]0xE9 + " ' `$ %USERNAME% $suffix")
$stateRoot = Join-Path $root 'state'
$mount = Join-Path $root 'mounted'
$powershell = Join-Path $env:SystemRoot 'System32\WindowsPowerShell\v1.0\powershell.exe'
$account = $null
$taskName = $null
$taskArguments = $null
$service = New-Object -ComObject Schedule.Service
$service.Connect()
$worker = Join-Path $root 'worker.ps1'
$cleanupFailures = [Collections.Generic.List[string]]::new()
function User-Action([string]$Operation) {
    $info = [Diagnostics.ProcessStartInfo]::new()
    $info.FileName = $powershell
    $info.Arguments = '-NoLogo -NoProfile -NonInteractive -ExecutionPolicy Bypass -File "' + $worker + '" -Operation ' + $Operation
    $info.UseShellExecute = $false
    $info.CreateNoWindow = $true
    $info.UserName = $userName
    $info.Domain = $env:COMPUTERNAME
    $info.Password = $password
    $info.LoadUserProfile = $true
    $info.WorkingDirectory = $root
    $info.RedirectStandardInput = $true
    $info.RedirectStandardOutput = $true
    $info.RedirectStandardError = $true
    $process = [Diagnostics.Process]::Start($info)
    try {
        $output = $process.StandardOutput.ReadToEndAsync()
        $errors = $process.StandardError.ReadToEndAsync()
        if ($Operation -eq 'Install') {
            # The receiving StreamReader recognizes the encoding preamble from
            # Windows PowerShell's console-derived StandardInput writer.
            $process.StandardInput.WriteLine($plainPassword)
        }
        $process.StandardInput.Close()
        if (-not $process.WaitForExit(90000)) { $process.Kill(); throw "Timed out in disposable-user $Operation" }
        $text = $output.GetAwaiter().GetResult()
        $errorText = $errors.GetAwaiter().GetResult()
        if ($process.ExitCode -ne 0) { throw "Disposable-user $Operation failed: $errorText $text" }
        return $text
    } finally { $process.Dispose() }
}
try {
    $account = New-LocalUser -Name $userName -Password $password -Description 'Disposable S3Drive native acceptance account'
    if (-not (Get-LocalGroupMember -SID 'S-1-5-32-545' | Where-Object SID -eq $account.SID)) {
        Add-LocalGroupMember -SID 'S-1-5-32-545' -Member $account
    }
    $taskName = "S3Drive-$($account.SID.Value)-acceptance"
    $null = [IO.Directory]::CreateDirectory($root)
    $acl = [Security.AccessControl.DirectorySecurity]::new()
    $acl.SetOwner($runner.User)
    $acl.SetAccessRuleProtection($true,$false)
    foreach ($allowed in @($runner.User.Value,$account.SID.Value,'S-1-5-18')) {
        $acl.AddAccessRule([Security.AccessControl.FileSystemAccessRule]::new([Security.Principal.SecurityIdentifier]::new($allowed), 'FullControl', 'ContainerInherit,ObjectInherit', 'None', 'Allow'))
    }
    Set-Acl -LiteralPath $root -AclObject $acl
    Copy-Item -LiteralPath $Executable -Destination (Join-Path $root 's3drive.exe')
    Copy-Item -LiteralPath $Config -Destination (Join-Path $root 'drive.toml')
    Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'windows-session.ps1') -Destination (Join-Path $root 'source-manager.ps1')
    # All worker paths are derived from its own private directory. The account
    # password arrives through a pipe, never a file, environment or argv string.
    @'
param([string]$Operation)
$ErrorActionPreference='Stop'
Set-StrictMode -Version Latest
$root=$PSScriptRoot
$state=Join-Path $root 'state'
$mount=Join-Path $root 'mounted'
$manager=Join-Path $root 'source-manager.ps1'
function Manage([string]$Action) {
    $output=& $manager -Action $Action -Name acceptance -StateDirectory $state
    if ($LASTEXITCODE -ne 0) { throw "Manager $Action failed" }
    return $output
}
try {
    switch ($Operation) {
        'Install' {
            $reader=[IO.StreamReader]::new([Console]::OpenStandardInput(),[Text.Encoding]::UTF8,$true)
            try { $received=$reader.ReadLine() } finally { $reader.Dispose() }
            if (-not $received.StartsWith('S3d!aA9-') -or $received.Length -ne 40) { throw 'Disposable credential pipe changed the fixture bytes' }
            $secure=ConvertTo-SecureString $received -AsPlainText -Force
            $received=$null
            $identity=[Security.Principal.WindowsIdentity]::GetCurrent()
            $credential=[Management.Automation.PSCredential]::new($identity.Name,$secure)
            $aws=Join-Path $env:USERPROFILE '.aws'
            $null=[IO.Directory]::CreateDirectory($aws)
            [IO.File]::WriteAllText((Join-Path $aws 'credentials'), "[fixture]`naws_access_key_id=TESTPROFILE`naws_secret_access_key=TESTPROFILESECRET`n")
            [IO.File]::WriteAllText((Join-Path $aws 'config'), '')
            & $manager -Action Install -Name acceptance -StateDirectory $state -Executable (Join-Path $root 's3drive.exe') -Config (Join-Path $root 'drive.toml') -MountPoint $mount -Credential $credential
            if ($LASTEXITCODE -ne 0) { throw 'Install failed' }
            $null=Manage Start
        }
        'Probe' {
            $deadline=[DateTime]::UtcNow.AddSeconds(45)
            $readme=Join-Path $mount 'other\readme'
            while ($true) {
                try { $read=[IO.File]::ReadAllText($readme); break }
                catch { if ([DateTime]::UtcNow -gt $deadline) { throw }; Start-Sleep -Milliseconds 200 }
            }
            if ($read -ne 'other bucket') { throw 'Wrong remote bytes from separate logon' }
            $unicode=Join-Path $mount ('docs\nested\a%2Fb + caf'+[char]0xE9+'.txt')
            if ([IO.File]::ReadAllText($unicode) -ne 'literal key bytes') { throw 'Unicode file read failed across logons' }
            $before=Get-Content -LiteralPath (Join-Path $state 'runtime.json') -Raw -Encoding UTF8 | ConvertFrom-Json
            1..3 | ForEach-Object { $null=Manage Start }
            Start-Sleep -Milliseconds 300
            $after=Get-Content -LiteralPath (Join-Path $state 'runtime.json') -Raw -Encoding UTF8 | ConvertFrom-Json
            if ($before.child_pid -ne $after.child_pid -or $before.supervisor_pid -ne $after.supervisor_pid) { throw 'Duplicate start replaced daemon' }
            $current=[Security.Principal.WindowsIdentity]::GetCurrent().User.Value
            $logon=(& (Join-Path $env:SystemRoot 'System32\whoami.exe') /logonid | Out-String).Trim()
            if ($before.owner_sid -ne $current -or $logon -eq $before.logon -or $logon -notmatch 'S-1-5-5-' -or $before.logon -notmatch 'S-1-5-5-') { throw 'Independent same-account logon not established' }
            $status=Manage Status | Out-String | ConvertFrom-Json
            if (-not $status.supervisor_running -or -not $status.mount_path_exists) { throw 'Incorrect mount status' }
            $wrote=$false
            try { [IO.File]::WriteAllText((Join-Path $mount 'other\new'), 'forbidden'); $wrote=$true } catch {}
            if ($wrote) { throw 'Session mount permitted write' }
            @{ caller_logon=$logon; supervisor_logon=$before.logon; child_pid=$before.child_pid; owner_sid=$current } | ConvertTo-Json -Compress
        }
        'Remove' {
            $null=Manage Remove
            if ([IO.Directory]::Exists($mount)) { throw 'Directory mount remains after removal' }
            if (-not [IO.File]::Exists((Join-Path $root 'drive.toml')) -or -not [IO.File]::Exists((Join-Path $env:USERPROFILE '.aws\credentials'))) { throw 'Removal deleted config or credentials' }
        }
        'Diagnostics' {
            if ([IO.File]::Exists((Join-Path $state 'startup.json'))) { Manage Status }
            if ([IO.File]::Exists((Join-Path $state 'session.log'))) { Get-Content -LiteralPath (Join-Path $state 'session.log') -Tail 30 }
        }
    }
} catch { [Console]::Error.WriteLine($_.Exception.Message); exit 1 }
'@ | Set-Content -LiteralPath $worker -Encoding UTF8
    $null = User-Action Install
    # The installer process (and its independent logon) has exited. Task
    # Scheduler must keep the mount alive without that process or console.
    $first = User-Action Probe | ConvertFrom-Json
    $second = User-Action Probe | ConvertFrom-Json
    if ($first.caller_logon -eq $second.caller_logon -or $first.child_pid -ne $second.child_pid) {
        throw 'Repeated independent logons did not access the same mount'
    }
    $unexpectedRead=$false
    try { $null=[IO.File]::ReadAllText((Join-Path $mount 'other\readme')); $unexpectedRead=$true } catch {}
    if ($unexpectedRead) { throw 'A different account could read the owner-only mount' }
    $task = $service.GetFolder('\').GetTask($taskName)
    $definition = $task.Definition
    if ($definition.Principal.LogonType -ne 1 -or $definition.Principal.RunLevel -ne 0 -or
        $definition.Settings.ExecutionTimeLimit -ne 'PT0S' -or $definition.Settings.MultipleInstances -ne 2 -or
        $definition.Settings.AllowHardTerminate -or $definition.Triggers.Count -ne 1 -or
        $definition.Triggers.Item(1).Type -ne 9) { throw 'Unexpected startup/lifetime settings' }
    # No copy of the disposable password may escape into persisted task/state.
    if ($task.Xml.Contains($plainPassword)) { throw 'Password leaked into task definition' }
    $null = User-Action Remove
    $removed=$false
    try { $null=$service.GetFolder('\').GetTask($taskName) }
    catch { if ($_.Exception.GetBaseException().HResult -eq -2147024894) { $removed=$true } else { throw } }
    if (-not $removed) { throw 'Task remains after removal' }
    Write-Output 'PASS: non-admin registration, independent same-account logons, exact S3 and Unicode reads, duplicate starts, retained mount and removal'
} catch {
    if ($null -ne $account -and [IO.File]::Exists($worker)) {
        try { Write-Output (User-Action Diagnostics) } catch { Write-Warning 'Could not collect isolated session diagnostics' }
    }
    throw
} finally {
    # Cleanup is restricted to the randomly named account, task and directory
    # made here. Force termination is a test-failure cleanup, never app behavior.
    if ($taskName) {
        try {
            $task=$service.GetFolder('\').GetTask($taskName)
            $taskArguments=$task.Definition.Actions.Item(1).Arguments
            $task.Enabled=$false
            $service.GetFolder('\').DeleteTask($taskName,0)
        } catch { if ($_.Exception.GetBaseException().HResult -ne -2147024894) { $cleanupFailures.Add('Could not delete disposable task') } }
    }
    if ([IO.Directory]::Exists($root)) {
        foreach ($process in Get-CimInstance Win32_Process -Filter "Name='powershell.exe' OR Name='s3drive.exe'") {
            if (($process.ExecutablePath -eq (Join-Path $root 's3drive.exe')) -or
                ($taskArguments -and $process.CommandLine -and $process.CommandLine.Contains($taskArguments))) {
                Stop-Process -Id $process.ProcessId -Force -ErrorAction SilentlyContinue
            }
        }
    }
    $unmounted=$false
    $deadline=[DateTime]::UtcNow.AddSeconds(15)
    while ([DateTime]::UtcNow -lt $deadline) {
        try { $null=[IO.File]::GetAttributes($mount) }
        catch {
            if ($_.Exception.GetBaseException() -is [IO.FileNotFoundException] -or
                $_.Exception.GetBaseException() -is [IO.DirectoryNotFoundException]) { $unmounted=$true; break }
        }
        Start-Sleep -Milliseconds 100
    }
    if (-not $unmounted) { $cleanupFailures.Add('Mountpoint still exists; preserving the isolated test directory') }
    if ($null -ne $account) {
        try {
            Get-CimInstance Win32_UserProfile | Where-Object SID -eq $account.SID.Value | Remove-CimInstance
            Remove-LocalUser -Name $userName
        } catch { $cleanupFailures.Add('Could not remove disposable user/profile') }
    }
    if ($unmounted -and [IO.Directory]::Exists($root)) {
        # The private manager state belongs to the disposable user. The elevated
        # runner takes ownership only of this isolated directory during cleanup.
        & takeown.exe /F $root /R /D Y | Out-Null
        & icacls.exe $root /grant "*$($runner.User.Value):(OI)(CI)F" /T /Q | Out-Null
        try { Remove-Item -LiteralPath $root -Recurse -Force }
        catch { $cleanupFailures.Add('Could not remove disposable test directory') }
    }
    $plainPassword=$null
    $password.Dispose()
    if ($cleanupFailures.Count) { throw ($cleanupFailures -join '; ') }
}
