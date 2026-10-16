#requires -Version 5.1
<#
Per-user Windows startup. Task Scheduler keeps the mount independent of the
launching terminal. Registration accepts a PSCredential only in local memory.
#>
[CmdletBinding()]
param(
    [Parameter(Mandatory)][ValidateSet('Install','Start','Status','Stop','Remove','Run')][string]$Action,
    [Parameter(Mandatory)][ValidatePattern('^[a-z0-9][a-z0-9-]{0,47}$')][string]$Name,
    [string]$Executable,
    [string]$Config,
    [string]$MountPoint,
    [string]$StateDirectory,
    [System.Management.Automation.PSCredential]$Credential
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
if ($env:OS -ne 'Windows_NT') { throw 'This launcher requires Windows' }
$identity = [Security.Principal.WindowsIdentity]::GetCurrent()
$sid = $identity.User.Value
$taskName = "S3Drive-$sid-$Name"
if (-not $StateDirectory) { $StateDirectory = Join-Path $env:LOCALAPPDATA "S3Drive\startup\$Name" }
$root = [IO.Path]::GetFullPath($StateDirectory)
$manifestPath = Join-Path $root 'startup.json'
$manager = Join-Path $root 'manager.ps1'
$stopFile = Join-Path $root 'stop'
$logFile = Join-Path $root 'session.log'
$powershell = Join-Path $env:SystemRoot 'System32\WindowsPowerShell\v1.0\powershell.exe'

function Assert-NoReparseAncestors([string]$Path) {
    $item = [IO.DirectoryInfo]::new($Path)
    while ($null -ne $item) {
        if ($item.Exists -and ($item.Attributes -band [IO.FileAttributes]::ReparsePoint)) {
            throw 'Startup paths cannot pass through junctions or symbolic links'
        }
        $item = $item.Parent
    }
}
function Assert-PrivateRoot {
    Assert-NoReparseAncestors $root
    if (-not [IO.Directory]::Exists($root)) { throw 'Startup registration directory does not exist' }
    $acl = Get-Acl -LiteralPath $root
    if ($acl.GetOwner([Security.Principal.SecurityIdentifier]).Value -ne $sid -or -not $acl.AreAccessRulesProtected) {
        throw 'Startup directory must have a private DACL owned by this account'
    }
    foreach ($rule in $acl.GetAccessRules($true,$true,[Security.Principal.SecurityIdentifier])) {
        if ($rule.AccessControlType -eq 'Allow' -and $rule.IdentityReference.Value -notin @($sid,'S-1-5-18')) {
            throw 'Startup directory grants access to another account'
        }
    }
}
function Quote-Argument([string]$Text) {
    # These are argv strings, not PowerShell source. Windows file names cannot
    # contain quotes; escaping trailing backslashes handles directory arguments.
    if ($Text.Contains('"') -or $Text.Contains([char]0)) { throw 'Invalid startup argument' }
    return '"' + [regex]::Replace($Text, '(\\+)$', '$1$1') + '"'
}
function Quote-Literal([string]$Text) { return "'" + $Text.Replace("'", "''") + "'" }
function Read-Manifest {
    Assert-PrivateRoot
    $value = Get-Content -LiteralPath $manifestPath -Raw -Encoding UTF8 | ConvertFrom-Json
    if ($value.version -ne 1 -or $value.owner_sid -ne $sid -or $value.task_name -ne $taskName) {
        throw 'Unsupported registration or different owning account'
    }
    return $value
}
function Scheduler {
    $service = New-Object -ComObject 'Schedule.Service'
    $service.Connect()
    return $service
}
function Registered-Task($Service) {
    try { return $Service.GetFolder('\').GetTask($taskName) }
    catch {
        if ($_.Exception.GetBaseException().HResult -eq -2147024894) { return $null }
        throw
    }
}
function Assert-Task($Task, $Value) {
    if ($null -eq $Task) { return }
    $definition = $Task.Definition
    $principal = $definition.Principal.UserId
    if ($principal -match '^S-1-') { $principalSid=[Security.Principal.SecurityIdentifier]::new($principal).Value }
    else { $principalSid=([Security.Principal.NTAccount]::new($principal)).Translate([Security.Principal.SecurityIdentifier]).Value }
    if ($principalSid -ne $sid -or $definition.Principal.LogonType -ne 1 -or $definition.Principal.RunLevel -ne 0) {
        throw 'Scheduled task owner or logon policy changed; refusing control'
    }
    if ($definition.RegistrationInfo.Description -cne $Value.description) {
        throw 'Scheduled task description changed; refusing control'
    }
    if ($definition.Actions.Count -ne 1 -or $definition.Actions.Item(1).Path -ine $powershell -or
        $definition.Actions.Item(1).Arguments -cne $Value.arguments -or
        $definition.Actions.Item(1).WorkingDirectory -ine $env:SystemRoot) {
        throw 'Scheduled task action changed; refusing control'
    }
}
function Is-Supervising {
    try {
        $handle = [IO.File]::Open((Join-Path $root 'supervisor.lock'), 'OpenOrCreate', 'ReadWrite', 'None')
        $handle.Dispose()
        return $false
    } catch [IO.IOException] {
        # Only a sharing violation establishes ownership; disk/path errors fail.
        if (($_.Exception.GetBaseException().HResult -band 65535) -eq 32) { return $true }
        throw
    }
}
function Stop-Mount($Task) {
    if ($null -eq $Task -and -not (Is-Supervising)) { return }
    [IO.File]::WriteAllText($stopFile, '')
    $deadline = [DateTime]::UtcNow.AddSeconds(40)
    while ((Is-Supervising) -or ($null -ne $Task -and $Task.State -in @(2,4))) {
        if ([DateTime]::UtcNow -gt $deadline) { throw 'Unmount is still pending; close open files and retry Stop' }
        Start-Sleep -Milliseconds 200
    }
    Remove-Item -LiteralPath $stopFile -Force -ErrorAction SilentlyContinue
}
function Log-Line([string]$Text) {
    [IO.File]::AppendAllText($logFile, ([DateTime]::UtcNow.ToString('o') + ' ' + $Text + [Environment]::NewLine))
}
function Run-Mount($Value) {
    try { $ownership = [IO.File]::Open((Join-Path $root 'supervisor.lock'), 'OpenOrCreate', 'ReadWrite', 'None') }
    catch [IO.IOException] {
        if (($_.Exception.GetBaseException().HResult -band 65535) -eq 32) { return 0 }
        throw
    }
    $child = $null
    try {
        if ([IO.File]::Exists($stopFile)) { return 0 }
        if ([IO.File]::Exists($logFile) -and (Get-Item -LiteralPath $logFile).Length -gt 1048576) {
            Move-Item -LiteralPath $logFile -Destination (Join-Path $root 'session.previous.log') -Force
        }
        $processInfo = [Diagnostics.ProcessStartInfo]::new()
        $processInfo.FileName = $Value.executable
        $processInfo.Arguments = 'mount ' + (Quote-Argument $Value.config) + ' ' + (Quote-Argument $Value.mountpoint) + ' --unmount-on-stdin-close'
        $processInfo.WorkingDirectory = $root
        $processInfo.UseShellExecute = $false
        $processInfo.CreateNoWindow = $true
        $processInfo.RedirectStandardInput = $true
        $processInfo.RedirectStandardError = $true
        $processInfo.RedirectStandardOutput = $true
        $child = [Diagnostics.Process]::Start($processInfo)
        $output = $child.StandardOutput.ReadToEndAsync()
        $line = $child.StandardError.ReadLineAsync()
        $stopping = $false
        $logon = (& (Join-Path $env:SystemRoot 'System32\whoami.exe') /logonid | Out-String).Trim()
        @{ supervisor_pid=$PID; child_pid=$child.Id; owner_sid=$sid; logon=$logon } |
            ConvertTo-Json | Set-Content -LiteralPath (Join-Path $root 'runtime.json') -Encoding UTF8
        Log-Line 'Mount process started'
        while (-not $child.HasExited) {
            if ($null -ne $line -and $line.IsCompleted) {
                $text = $line.GetAwaiter().GetResult()
                if ($null -eq $text) { $line = $null }
                else { Log-Line $text; $line = $child.StandardError.ReadLineAsync() }
            }
            if (-not $stopping -and [IO.File]::Exists($stopFile)) {
                $stopping = $true
                $child.StandardInput.Close()
                Log-Line 'Graceful unmount requested'
            }
            Start-Sleep -Milliseconds 100
        }
        $child.WaitForExit()
        if ($null -ne $line) {
            $text = $line.GetAwaiter().GetResult()
            if ($null -ne $text) { Log-Line $text }
            $remaining = $child.StandardError.ReadToEnd()
            if ($remaining) { Log-Line $remaining }
        }
        $null = $output.GetAwaiter().GetResult()
        Log-Line "Mount process exited $($child.ExitCode)"
        return $child.ExitCode
    } finally {
        if ($null -ne $child) {
            if (-not $child.HasExited) {
                $child.StandardInput.Close()
                # Never force-kill a mount whose open descriptors delay teardown.
                $child.WaitForExit()
            }
            $child.Dispose()
        }
        Remove-Item -LiteralPath $stopFile -Force -ErrorAction SilentlyContinue
        $ownership.Dispose()
    }
}

try {
    if ($Action -eq 'Install') {
        if (-not $Executable -or -not $Config -or -not $MountPoint) { throw 'Install requires Executable, Config and MountPoint' }
        $Executable = (Get-Item -LiteralPath $Executable).FullName
        $Config = (Get-Item -LiteralPath $Config).FullName
        if ($MountPoint -notmatch '^[A-Za-z]:[\\/]') { throw 'MountPoint must be an absolute local directory path' }
        $MountPoint = [IO.Path]::GetFullPath($MountPoint)
        if ($MountPoint -notmatch '^[A-Za-z]:\\.+' -or [IO.File]::Exists($MountPoint) -or [IO.Directory]::Exists($MountPoint)) {
            throw 'Use a new absolute NTFS directory mountpoint, not an existing directory or drive letter'
        }
        if (-not [IO.Directory]::Exists([IO.Path]::GetDirectoryName($MountPoint))) { throw 'Mountpoint parent must already exist' }
        Assert-NoReparseAncestors $MountPoint
        Assert-NoReparseAncestors $root
        foreach ($path in @($Executable,$Config,$root)) {
            if ($path.TrimEnd('\').Equals($MountPoint.TrimEnd('\'),[StringComparison]::OrdinalIgnoreCase) -or
                $path.StartsWith($MountPoint.TrimEnd('\')+'\',[StringComparison]::OrdinalIgnoreCase)) {
                throw 'Executable, configuration and startup state must stay outside the mount'
            }
        }
        & $Executable config-check $Config
        if ($LASTEXITCODE -ne 0) { throw 'Configuration validation failed' }
        $service = Scheduler
        if ($null -ne (Registered-Task $service)) { throw 'Scheduled task already exists; remove the old registration first' }
        if ([IO.Directory]::Exists($root) -or [IO.File]::Exists($root)) { throw 'State directory already exists; use a new directory or remove the old registration data' }
        if ($null -eq $Credential) {
            $Credential = Get-Credential -UserName $identity.Name -Message 'Windows account password for independent S3Drive startup (not an S3 key)'
        }
        if ($null -eq $Credential) { throw 'Registration cancelled' }
        $credentialSid = ([Security.Principal.NTAccount]::new($Credential.UserName)).Translate([Security.Principal.SecurityIdentifier]).Value
        if ($credentialSid -ne $sid) { throw 'Register startup as the same account that will access the drive' }
        $null = [IO.Directory]::CreateDirectory($root)
        $acl = [Security.AccessControl.DirectorySecurity]::new()
        $acl.SetOwner($identity.User)
        $acl.SetAccessRuleProtection($true,$false)
        foreach ($allowed in @($sid,'S-1-5-18')) {
            $rule = [Security.AccessControl.FileSystemAccessRule]::new([Security.Principal.SecurityIdentifier]::new($allowed), 'FullControl', 'ContainerInherit,ObjectInherit', 'None', 'Allow')
            $acl.AddAccessRule($rule)
        }
        Set-Acl -LiteralPath $root -AclObject $acl
        Assert-PrivateRoot
        Copy-Item -LiteralPath $PSCommandPath -Destination $manager
        # Task Scheduler expands %ENV% in action arguments. An encoded command
        # preserves literal path characters; all inserted values are quoted
        # PowerShell literals. This is encoding, not encryption; no secrets here.
        $invocation = '& ' + (Quote-Literal $manager) + ' -Action Run -Name ' + (Quote-Literal $Name) + ' -StateDirectory ' + (Quote-Literal $root) + '; exit $LASTEXITCODE'
        $arguments = '-NoLogo -NoProfile -NonInteractive -ExecutionPolicy Bypass -EncodedCommand ' + [Convert]::ToBase64String([Text.Encoding]::Unicode.GetBytes($invocation))
        $description = "S3Drive startup v1; owner $sid; registration $Name"
        $value = @{ version=1; owner_sid=$sid; task_name=$taskName; executable=$Executable; config=$Config; mountpoint=$MountPoint; arguments=$arguments; description=$description }
        $value | ConvertTo-Json | Set-Content -LiteralPath $manifestPath -Encoding UTF8
        $definition = $service.NewTask(0)
        $definition.RegistrationInfo.Description = $description
        $definition.Principal.UserId = $identity.Name
        $definition.Principal.LogonType = 1 # TASK_LOGON_PASSWORD, independent batch logon
        $definition.Principal.RunLevel = 0 # least privilege
        $definition.Settings.Enabled = $true
        $definition.Settings.ExecutionTimeLimit = 'PT0S'
        $definition.Settings.MultipleInstances = 2 # ignore duplicate starts
        $definition.Settings.DisallowStartIfOnBatteries = $false
        $definition.Settings.StopIfGoingOnBatteries = $false
        $definition.Settings.AllowHardTerminate = $false
        $definition.Settings.RestartCount = 3
        $definition.Settings.RestartInterval = 'PT1M'
        $trigger = $definition.Triggers.Create(9) # TASK_TRIGGER_LOGON
        $trigger.UserId = $identity.Name
        $trigger.Enabled = $true
        $exec = $definition.Actions.Create(0)
        $exec.Path = $powershell
        $exec.Arguments = $arguments
        $exec.WorkingDirectory = $env:SystemRoot
        # COM receives the password in memory. It is not a process argument,
        # script, log, manifest field or repository secret.
        $plainPassword = $Credential.GetNetworkCredential().Password
        try { $registered = $service.GetFolder('\').RegisterTaskDefinition($taskName,$definition,2,$identity.Name,$plainPassword,1,$null) }
        finally { $plainPassword = $null; $Credential = $null }
        Assert-Task $registered $value
        Write-Output "Installed $taskName. Use Start to mount now; future account logons trigger it."
    } else {
        $value = Read-Manifest
        if ($Action -eq 'Run') { exit (Run-Mount $value) }
        $service = Scheduler
        $task = Registered-Task $service
        Assert-Task $task $value
        switch ($Action) {
            'Start' {
                if ($null -eq $task) { throw 'Scheduled task is missing; reinstall startup' }
                if ($task.State -notin @(2,4) -and -not (Is-Supervising)) {
                    Remove-Item -LiteralPath $stopFile -Force -ErrorAction SilentlyContinue
                }
                $null = $task.Run($null)
                Write-Output 'Startup requested; use Status and session.log to confirm the mount'
            }
            'Status' {
                $states=@('unknown','disabled','queued','ready','running')
                @{ task_registered=($null -ne $task); task_state=$(if ($task) {$states[$task.State]} else {'missing'});
                   last_task_result=$(if ($task) {$task.LastTaskResult} else {$null});
                   supervisor_running=(Is-Supervising); mount_path_exists=[IO.Directory]::Exists($value.mountpoint);
                   mountpoint=$value.mountpoint; log=$logFile } | ConvertTo-Json
            }
            'Stop' { Stop-Mount $task; Write-Output 'Mount stopped' }
            'Remove' {
                if ($null -ne $task) { $task.Enabled = $false }
                Stop-Mount $task
                if ($null -ne $task) { $service.GetFolder('\').DeleteTask($taskName,0) }
                foreach ($file in @('startup.json','manager.ps1','stop','runtime.json')) {
                    Remove-Item -LiteralPath (Join-Path $root $file) -Force -ErrorAction SilentlyContinue
                }
                Write-Output 'Startup removed; configuration, credentials, parent directory and logs retained'
            }
        }
    }
} catch {
    [Console]::Error.WriteLine($_.Exception.Message)
    exit 1
}
