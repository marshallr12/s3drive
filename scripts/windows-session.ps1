#requires -Version 5.1
<#
Per-user Windows startup. Task Scheduler keeps the mount independent of the
launching terminal. Registration accepts a PSCredential only in local memory.

Mount registrations have two modes:
- drive (default): a drive letter with the S3Drive icon, mounted in the
  signed-in user's own logon session at each sign-in, like Google Drive. The
  task uses the interactive token, so no password is stored or requested.
- directory: an NTFS directory path, mounted in a separate batch logon that
  stays visible across logons and SSH. It needs the account password.
#>
[CmdletBinding()]
param(
    [Parameter(Mandatory)][ValidateSet('Install','Start','Status','Stop','Remove','Run')][string]$Action,
    [Parameter(Mandatory)][ValidatePattern('^[a-z0-9][a-z0-9-]{0,47}$')][string]$Name,
    [string]$Executable,
    [string]$Config,
    [string]$MountPoint,
    [string]$Icon,
    [string]$StateDirectory,
    [ValidateSet('Mount','Authority')][string]$Component = 'Mount',
    [string]$AuthorityDirectory,
    [string]$AuthorityVolume,
    [ValidateSet('exact-utf8-v1','portable-collision-v1')][string]$AuthorityNamePolicy,
    [string]$AuthorityTokenFile,
    [string]$AuthorityListen,
    [string]$TlsCert,
    [string]$TlsKey,
    [System.Management.Automation.PSCredential]$Credential
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
if ($env:OS -ne 'Windows_NT') { throw 'This launcher requires Windows' }
$identity = [Security.Principal.WindowsIdentity]::GetCurrent()
$sid = $identity.User.Value
$prefix = if ($Component -eq 'Authority') { 'S3DriveAuthority' } else { 'S3Drive' }
$taskName = "$prefix-$sid-$Name"
if (-not $StateDirectory) {
    $namespace = if ($Component -eq 'Authority') { 'authority-startup' } else { 'startup' }
    $StateDirectory = Join-Path $env:LOCALAPPDATA "S3Drive\$namespace\$Name"
}
$root = [IO.Path]::GetFullPath($StateDirectory)
$manifestPath = Join-Path $root 'startup.json'
$manager = Join-Path $root 'manager.ps1'
$stopFile = Join-Path $root 'stop'
$logFile = Join-Path $root 'session.log'
$powershell = Join-Path $env:SystemRoot 'System32\WindowsPowerShell\v1.0\powershell.exe'
# Per-user Explorer drive icon overrides; HKCU needs no elevation.
$driveIconRoot = 'Software\Classes\Applications\Explorer.exe\Drives'

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
function Authority-Arguments($Authority) {
    $result = @('--directory',$Authority.directory,'--expect-volume',$Authority.volume,
                '--name-policy',$Authority.name_policy,'--token-file',$Authority.token_file,'--listen',$Authority.listen)
    if ($Authority.tls_cert) { $result += @('--tls-cert',$Authority.tls_cert,'--tls-key',$Authority.tls_key) }
    return $result
}
function Is-DriveMode($Value) { return $Value.version -eq 3 }
function Assert-DriveManifest($Value) {
    $fields = @('version','mode','owner_sid','task_name','executable','config','mountpoint','launcher','icon','arguments','description')
    $keys = @($Value.PSObject.Properties.Name)
    if ($keys.Count -ne $fields.Count) { throw 'Unsupported drive manifest fields' }
    foreach ($field in $fields) { if ($keys -cnotcontains $field) { throw 'Incomplete drive manifest' } }
    if ($Value.mode -cne 'drive' -or $Value.mountpoint -cnotmatch '^[A-Z]:$') { throw 'Invalid drive registration' }
}
function Registered-DriveLetters {
    # Letters claimed by this account's other default-location registrations,
    # including ones that are not mounted right now.
    $parent = Join-Path $env:LOCALAPPDATA 'S3Drive\startup'
    foreach ($directory in @(Get-ChildItem -LiteralPath $parent -Directory -ErrorAction SilentlyContinue)) {
        $file = Join-Path $directory.FullName 'startup.json'
        try { $point = [string](Get-Content -LiteralPath $file -Raw -Encoding UTF8 | ConvertFrom-Json).mountpoint } catch { continue }
        if ($point -match '^[A-Za-z]:$') { $point.Substring(0,1).ToUpperInvariant() }
    }
}
function Used-DriveLetters {
    # Letters visible in this logon session, remembered network drives and
    # other S3Drive registrations.
    $used = @([IO.Directory]::GetLogicalDrives() | ForEach-Object { $_.Substring(0,1).ToUpperInvariant() })
    $used += @(Get-ChildItem -LiteralPath 'HKCU:\Network' -ErrorAction SilentlyContinue | ForEach-Object { $_.PSChildName.ToUpperInvariant() })
    return $used + @(Registered-DriveLetters)
}
function Select-DriveLetter([string]$Requested) {
    # Honour an explicit letter, else prefer S: then search downward from Z:.
    $used = @(Used-DriveLetters)
    if ($Requested) {
        $letter = $Requested.Substring(0,1).ToUpperInvariant()
        if ($letter -in @('A','B','C') -or $used -contains $letter) { throw "Drive ${letter}: is in use or reserved; choose another letter or omit MountPoint" }
        return "${letter}:"
    }
    foreach ($letter in @('S') + @([char[]](90..68) | ForEach-Object { [string]$_ })) {
        if ($used -notcontains $letter) { return "${letter}:" }
    }
    throw 'No free drive letter; pass a directory MountPoint instead'
}
function Assert-DriveIconFree([string]$Drive, [string]$IconPath) {
    # Never take over a drive icon that another program or the user set.
    $key = [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey("$driveIconRoot\$($Drive.Substring(0,1))\DefaultIcon")
    if ($null -eq $key) { return }
    try { $existing = $key.GetValue('') } finally { $key.Dispose() }
    if ($null -ne $existing -and $existing -cne $IconPath) { throw "Drive ${Drive} already has a custom icon; choose another letter" }
}
function Set-DriveIcon([string]$Drive, [string]$IconPath) {
    # Explorer reads this per-user override when it shows the drive letter.
    $key = [Microsoft.Win32.Registry]::CurrentUser.CreateSubKey("$driveIconRoot\$($Drive.Substring(0,1))\DefaultIcon")
    try { $key.SetValue('', $IconPath) } finally { $key.Dispose() }
}
function Remove-DriveIcon([string]$Drive, [string]$IconPath) {
    # Delete only the exact icon value this registration wrote, then the empty letter key.
    $letterKey = "$driveIconRoot\$($Drive.Substring(0,1))"
    $key = [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey("$letterKey\DefaultIcon")
    if ($null -eq $key) { return }
    try { $owned = ($key.GetValue('') -ceq $IconPath) -and $key.ValueCount -eq 1 -and $key.SubKeyCount -eq 0 } finally { $key.Dispose() }
    if (-not $owned) { return }
    [Microsoft.Win32.Registry]::CurrentUser.DeleteSubKey("$letterKey\DefaultIcon")
    $parent = [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey($letterKey)
    try { $empty = $parent.ValueCount -eq 0 -and $parent.SubKeyCount -eq 0 } finally { $parent.Dispose() }
    if ($empty) { [Microsoft.Win32.Registry]::CurrentUser.DeleteSubKey($letterKey) }
}
function Read-Manifest {
    Assert-PrivateRoot
    $value = Get-Content -LiteralPath $manifestPath -Raw -Encoding UTF8 | ConvertFrom-Json
    $kind = if ($value.version -in @(1,3)) { 'Mount' } elseif ($value.version -eq 2) { $value.component } else { '' }
    if (($value.version -eq 2 -and $kind -ine 'Authority') -or $kind -ine $Component -or $value.owner_sid -ne $sid -or $value.task_name -ne $taskName) {
        throw 'Unsupported registration or different owning account'
    }
    if ($kind -ieq 'Authority') {
        $manifestFields = @('version','component','owner_sid','task_name','executable','authority','arguments','description')
        $manifestKeys = @($value.PSObject.Properties.Name)
        if ($manifestKeys.Count -ne $manifestFields.Count) { throw 'Unsupported authority manifest fields' }
        foreach ($field in $manifestFields) { if ($manifestKeys -cnotcontains $field) { throw 'Incomplete authority manifest' } }
        $fields = @('directory','volume','name_policy','token_file','listen','tls_cert','tls_key')
        $actual = @($value.authority.PSObject.Properties.Name)
        if ($actual.Count -ne $fields.Count) { throw 'Unsupported authority registration fields' }
        foreach ($field in $fields) {
            if ($actual -cnotcontains $field) { throw 'Incomplete authority registration' }
            if ($field -notin @('tls_cert','tls_key') -and
                ($value.authority.$field -isnot [string] -or -not $value.authority.$field)) {
                throw 'Invalid pinned authority registration'
            }
        }
        if ([bool]$value.authority.tls_cert -ne [bool]$value.authority.tls_key) { throw 'Incomplete authority TLS registration' }
    }
    if (Is-DriveMode $value) { Assert-DriveManifest $value }
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
    # Drive mode: interactive token (3) in the user's session. Otherwise password (1).
    $logonType = if (Is-DriveMode $Value) { 3 } else { 1 }
    $actionPath = if (Is-DriveMode $Value) { $Value.launcher } else { $powershell }
    if ($principalSid -ne $sid -or $definition.Principal.LogonType -ne $logonType -or $definition.Principal.RunLevel -ne 0) {
        throw 'Scheduled task owner or logon policy changed; refusing control'
    }
    if ($definition.RegistrationInfo.Description -cne $Value.description) {
        throw 'Scheduled task description changed; refusing control'
    }
    if ($definition.Actions.Count -ne 1 -or $definition.Actions.Item(1).Path -ine $actionPath -or
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
        if ([DateTime]::UtcNow -gt $deadline) {
            if ($Component -eq 'Authority') { throw 'Coordinator shutdown is still pending; inspect session.log and retry Stop' }
            throw 'Unmount is still pending; close open files and retry Stop'
        }
        Start-Sleep -Milliseconds 200
    }
    Remove-Item -LiteralPath $stopFile -Force -ErrorAction SilentlyContinue
}
function Log-Line([string]$Text) {
    [IO.File]::AppendAllText($logFile, ([DateTime]::UtcNow.ToString('o') + ' ' + $Text + [Environment]::NewLine))
}
function Run-Child($Value) {
    $child = $null
    try {
        if ([IO.File]::Exists($stopFile)) { return 0 }
        if ([IO.File]::Exists($logFile) -and (Get-Item -LiteralPath $logFile).Length -gt 1048576) {
            Move-Item -LiteralPath $logFile -Destination (Join-Path $root 'session.previous.log') -Force
        }
        $processInfo = [Diagnostics.ProcessStartInfo]::new()
        $processInfo.FileName = $Value.executable
        if ($Component -eq 'Authority') {
            $processInfo.Arguments = 'authority-serve ' + ((Authority-Arguments $Value.authority | ForEach-Object { Quote-Argument $_ }) -join ' ') + ' --stop-on-stdin-close'
        } else {
            $processInfo.Arguments = 'mount ' + (Quote-Argument $Value.config) + ' ' + (Quote-Argument $Value.mountpoint) + ' --unmount-on-stdin-close'
        }
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
        Log-Line "$Component process started"
        # Read-only scheduling observations. Diagnostics must never turn a
        # successful child launch into a failed/retried launch.
        try {
            $supervisor = [Diagnostics.Process]::GetCurrentProcess()
            try { $supervisorPriority = $supervisor.PriorityClass.ToString() }
            finally { $supervisor.Dispose() }
            $childPriority = $child.PriorityClass.ToString()
            Log-Line "$Component scheduling: supervisor=$supervisorPriority child=$childPriority"
        } catch {
            try { Log-Line "$Component scheduling observation unavailable" } catch {}
        }
        while (-not $child.HasExited) {
            if ($null -ne $line -and $line.IsCompleted) {
                $text = $line.GetAwaiter().GetResult()
                if ($null -eq $text) { $line = $null }
                else { Log-Line $text; $line = $child.StandardError.ReadLineAsync() }
            }
            if (-not $stopping -and [IO.File]::Exists($stopFile)) {
                $stopping = $true
                $child.StandardInput.Close()
                Log-Line "Graceful $Component shutdown requested"
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
        Log-Line "$Component process exited $($child.ExitCode)"
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
    }
}
function Run-Mount($Value) {
    try { $ownership = [IO.File]::Open((Join-Path $root 'supervisor.lock'), 'OpenOrCreate', 'ReadWrite', 'None') }
    catch [IO.IOException] {
        if (($_.Exception.GetBaseException().HResult -band 65535) -eq 32) { return 0 }
        throw
    }
    try {
        # Keep one supervisor/lock through child recovery. Scheduler failure
        # settings are not evidence that a nonzero child action will restart.
        for ($attempt=0; $attempt -lt 4; $attempt++) {
            if ([IO.File]::Exists($stopFile)) { return 0 }
            try { $code=[int](Run-Child $Value) }
            catch { Log-Line ("$Component child start failed: "+$_.Exception.Message); $code=1 }
            if ([IO.File]::Exists($stopFile) -or $code -eq 0) { return 0 }
            if ($attempt -eq 3) { Log-Line "$Component retry budget exhausted; original state retained"; return $code }
            Log-Line ("$Component retry "+($attempt+2)+"/4 in 10 seconds; original state retained")
            for ($pause=0; $pause -lt 50; $pause++) {
                if ([IO.File]::Exists($stopFile)) { return 0 }
                Start-Sleep -Milliseconds 200
            }
        }
    } finally {
        Remove-Item -LiteralPath $stopFile -Force -ErrorAction SilentlyContinue
        $ownership.Dispose()
    }
}

try {
    if ($Action -eq 'Install') {
        if (-not $Executable) { throw 'Install requires Executable' }
        $driveMode = $false
        $Executable = (Get-Item -LiteralPath $Executable).FullName
        $authority = $null
        if ($Component -eq 'Authority') {
            if (-not $AuthorityDirectory -or -not $AuthorityVolume -or -not $AuthorityNamePolicy -or
                -not $AuthorityTokenFile -or -not $AuthorityListen -or $Config -or $MountPoint -or
                ([bool]$TlsCert -ne [bool]$TlsKey)) { throw 'Authority install requires all authority settings, no mount options and a complete optional TLS pair' }
            if ($AuthorityDirectory -notmatch '^[A-Za-z]:[\\/]') { throw 'Authority directory must be an absolute local path' }
            $authority = @{directory=[IO.Path]::GetFullPath($AuthorityDirectory); volume=$AuthorityVolume;
                name_policy=$AuthorityNamePolicy;token_file=[IO.Path]::GetFullPath($AuthorityTokenFile);listen=$AuthorityListen;
                tls_cert=$(if($TlsCert){[IO.Path]::GetFullPath($TlsCert)}else{$null});tls_key=$(if($TlsKey){[IO.Path]::GetFullPath($TlsKey)}else{$null})}
            $authorityArgs = @(Authority-Arguments ([pscustomobject]$authority))
            & $Executable authority-check @authorityArgs
            if ($LASTEXITCODE -ne 0) { throw 'Existing pinned authority validation failed' }
            $protectedRoot = $authority.directory
            $protected = @($Executable,$root,$authority.token_file) + @($authority.tls_cert,$authority.tls_key | Where-Object {$_})
        } else {
            if (-not $Config -or $AuthorityDirectory -or $AuthorityVolume -or $AuthorityNamePolicy -or $AuthorityTokenFile -or $AuthorityListen -or $TlsCert -or $TlsKey) { throw 'Mount install requires Config and no authority options' }
            $Config = (Get-Item -LiteralPath $Config).FullName
            # No MountPoint, or a bare letter such as G:, selects the default drive mode.
            $driveMode = (-not $MountPoint) -or ($MountPoint -match '^[A-Za-z]:\\?$')
            if ($driveMode) {
                if ($Credential) { throw 'Drive-letter mode runs in your own sign-in session and does not use a password' }
                $MountPoint = Select-DriveLetter $MountPoint
                $installDirectory = Split-Path -Parent $Executable
                if (-not $Icon) { $Icon = Join-Path $installDirectory 's3drive.ico' }
                if ([IO.File]::Exists($Icon)) { $Icon = (Get-Item -LiteralPath $Icon).FullName; Assert-DriveIconFree $MountPoint $Icon }
                else { Write-Warning 'S3Drive icon not found; Explorer will show its default drive icon'; $Icon = $null }
                $launcher = Join-Path $installDirectory 's3drive-launch.exe'
                if (-not [IO.File]::Exists($launcher)) { Write-Warning 's3drive-launch.exe not found; a console window may flash at sign-in'; $launcher = $powershell }
                $protectedRoot = $MountPoint + '\'
            } else {
                if ($Icon) { throw 'Icon applies only to drive-letter mounts' }
                if ($MountPoint -notmatch '^[A-Za-z]:[\\/]') { throw 'MountPoint must be a drive letter or an absolute local directory path' }
                $MountPoint = [IO.Path]::GetFullPath($MountPoint)
                if ($MountPoint -notmatch '^[A-Za-z]:\\.+' -or [IO.File]::Exists($MountPoint) -or [IO.Directory]::Exists($MountPoint)) {
                    throw 'Use a new absolute NTFS directory mountpoint, not an existing directory'
                }
                if (-not [IO.Directory]::Exists([IO.Path]::GetDirectoryName($MountPoint))) { throw 'Mountpoint parent must already exist' }
                $protectedRoot = $MountPoint
            }
            $protected = @($Executable,$Config,$root)
            & $Executable config-check $Config
            if ($LASTEXITCODE -ne 0) { throw 'Configuration validation failed' }
        }
        Assert-NoReparseAncestors $protectedRoot
        Assert-NoReparseAncestors $root
        foreach ($path in $protected) {
            if ($path.TrimEnd('\').Equals($protectedRoot.TrimEnd('\'),[StringComparison]::OrdinalIgnoreCase) -or
                $path.StartsWith($protectedRoot.TrimEnd('\')+'\',[StringComparison]::OrdinalIgnoreCase)) {
                throw 'Executable, private inputs and startup state must stay outside the mounted/data directory'
            }
        }
        if ($authority -and $protectedRoot.StartsWith($root.TrimEnd('\')+'\',[StringComparison]::OrdinalIgnoreCase)) { throw 'Authority database and startup registration must be separate trees' }
        $service = Scheduler
        if ($null -ne (Registered-Task $service)) { throw 'Scheduled task already exists; remove the old registration first' }
        if ([IO.Directory]::Exists($root) -or [IO.File]::Exists($root)) { throw 'State directory already exists; use a new directory or remove the old registration data' }
        if (-not $driveMode) {
            if ($null -eq $Credential) {
                $Credential = Get-Credential -UserName $identity.Name -Message 'Windows account password for independent S3Drive startup (not an S3 key)'
            }
            if ($null -eq $Credential) { throw 'Registration cancelled' }
            $credentialSid = ([Security.Principal.NTAccount]::new($Credential.UserName)).Translate([Security.Principal.SecurityIdentifier]).Value
            if ($credentialSid -ne $sid) { throw 'Register startup as the same account that will access the drive' }
        }
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
        $invocation = '& ' + (Quote-Literal $manager) + ' -Action Run -Name ' + (Quote-Literal $Name) + ' -StateDirectory ' + (Quote-Literal $root)
        if ($authority) { $invocation += ' -Component Authority' }
        $invocation += '; exit $LASTEXITCODE'
        $arguments = '-NoLogo -NoProfile -NonInteractive -ExecutionPolicy Bypass -EncodedCommand ' + [Convert]::ToBase64String([Text.Encoding]::Unicode.GetBytes($invocation))
        if ($driveMode -and $launcher -eq $powershell) { $arguments = '-WindowStyle Hidden ' + $arguments }
        $description = "S3Drive startup v1; owner $sid; registration $Name"
        $value = @{ version=1; owner_sid=$sid; task_name=$taskName; executable=$Executable; config=$Config; mountpoint=$MountPoint; arguments=$arguments; description=$description }
        if ($driveMode) {
            $description = "S3Drive drive startup v3; owner $sid; registration $Name"
            $value = @{ version=3; mode='drive'; owner_sid=$sid; task_name=$taskName; executable=$Executable; config=$Config;
                mountpoint=$MountPoint; launcher=$launcher; icon=$Icon; arguments=$arguments; description=$description }
        }
        if ($authority) {
            $description = "S3Drive authority startup v2; owner $sid; registration $Name"
            $value = @{version=2;component='authority';owner_sid=$sid;task_name=$taskName;executable=$Executable;
                authority=$authority;arguments=$arguments;description=$description}
        }
        $value | ConvertTo-Json | Set-Content -LiteralPath $manifestPath -Encoding UTF8
        $definition = $service.NewTask(0)
        $definition.RegistrationInfo.Description = $description
        $definition.Principal.UserId = $identity.Name
        # Drive: TASK_LOGON_INTERACTIVE_TOKEN, so the letter is created in the
        # signed-in session Explorer uses. Otherwise TASK_LOGON_PASSWORD, an
        # independent batch logon whose directory mount spans logons and SSH.
        $definition.Principal.LogonType = $(if ($driveMode) { 3 } else { 1 })
        $definition.Principal.RunLevel = 0 # least privilege
        $definition.Settings.Enabled = $true
        $definition.Settings.AllowDemandStart = $true
        $definition.Settings.RunOnlyIfIdle = $false
        $definition.Settings.RunOnlyIfNetworkAvailable = $false
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
        $exec.Path = $(if ($driveMode) { $launcher } else { $powershell })
        $exec.Arguments = $arguments
        $exec.WorkingDirectory = $env:SystemRoot
        # COM receives the password in memory. It is not a process argument,
        # script, log, manifest field or repository secret.
        if ($driveMode) {
            # The interactive token needs no stored password.
            $registered = $service.GetFolder('\').RegisterTaskDefinition($taskName,$definition,2,$identity.Name,$null,3,$null)
        } else {
            $plainPassword = $Credential.GetNetworkCredential().Password
            try { $registered = $service.GetFolder('\').RegisterTaskDefinition($taskName,$definition,2,$identity.Name,$plainPassword,1,$null) }
            finally { $plainPassword = $null; $Credential = $null }
        }
        Assert-Task $registered $value
        if ($driveMode -and $Icon) { Set-DriveIcon $MountPoint $Icon }
        if ($driveMode) { Write-Output "Installed $taskName for drive $MountPoint. Use Start now; it mounts again at each sign-in." }
        else { Write-Output "Installed $taskName. Use Start now; future account logons trigger it." }
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
                $deadline=[DateTime]::UtcNow.AddSeconds(30)
                while (-not (Is-Supervising)) {
                    if ([DateTime]::UtcNow -gt $deadline) { throw "Supervisor did not start; scheduler result $($task.LastTaskResult); inspect session.log and Task Scheduler history" }
                    Start-Sleep -Milliseconds 100
                }
                Write-Output 'Supervisor running; use Status, session.log and an authenticated operation to confirm readiness'
            }
            'Status' {
                $states=@('unknown','disabled','queued','ready','running')
                $status = @{ task_registered=($null -ne $task); task_state=$(if ($task) {$states[$task.State]} else {'missing'});
                   last_task_result=$(if ($task) {$task.LastTaskResult} else {$null});
                   supervisor_running=(Is-Supervising); log=$logFile }
                if ($Component -eq 'Authority') {
                    $status.component='authority';$status.expected_volume=$value.authority.volume;
                    $status.listen=$value.authority.listen;$status.provider_access_verified=$false
                } else {
                    $drive = Is-DriveMode $value
                    $status.mode=$(if ($drive) {'drive'} else {'directory'})
                    $status.mount_path_exists=[IO.Directory]::Exists($(if ($drive) {$value.mountpoint + '\'} else {$value.mountpoint}))
                    $status.mountpoint=$value.mountpoint
                }
                $status | ConvertTo-Json
            }
            'Stop' { Stop-Mount $task; Write-Output "$Component stopped" }
            'Remove' {
                if ($null -ne $task) { $task.Enabled = $false }
                Stop-Mount $task
                if ($null -ne $task) { $service.GetFolder('\').DeleteTask($taskName,0) }
                if ((Is-DriveMode $value) -and $value.icon) { Remove-DriveIcon $value.mountpoint $value.icon }
                foreach ($file in @('startup.json','manager.ps1','stop','runtime.json')) {
                    Remove-Item -LiteralPath (Join-Path $root $file) -Force -ErrorAction SilentlyContinue
                }
                Write-Output 'Startup removed; configuration, credentials, parent directory and logs retained'
            }
        }
    }
} catch {
    $message=$_.Exception.Message
    if ($Action -eq 'Run') {
        try { Assert-PrivateRoot; Log-Line ("Supervisor failed: " + $message) } catch {}
    }
    [Console]::Error.WriteLine($message)
    exit 1
}
exit 0
