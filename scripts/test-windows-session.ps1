#requires -Version 5.1
# Explicit disposable-runner acceptance. Never run this against an existing
# user account: it creates and removes its own non-administrator local user.
param([Parameter(Mandatory)][string]$Executable, [string]$Config,
    [switch]$AuthorityOnly, [string]$AuthorityFixture, [string]$ManagedFixture)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
if (($AuthorityOnly -and -not $AuthorityFixture) -or (-not $AuthorityOnly -and -not $Config)) {
    throw 'Fixture requires Config for mounts, or AuthorityOnly with AuthorityFixture'
}
if ($ManagedFixture -and (-not $AuthorityOnly -or -not $AuthorityFixture)) { throw 'ManagedFixture requires AuthorityOnly and AuthorityFixture' }
$runner = [Security.Principal.WindowsIdentity]::GetCurrent()
if (-not ([Security.Principal.WindowsPrincipal]::new($runner)).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
    throw 'Native session acceptance requires an elevated disposable Windows runner'
}
$suffix = [Guid]::NewGuid().ToString('N').Substring(0,10)
$userName = "s3d$suffix"
$plainPassword = 'S3d!aA9-' + [Guid]::NewGuid().ToString('N')
$password = ConvertTo-SecureString $plainPassword -AsPlainText -Force
# All ancestors must be readable to the disposable user; another account's
# AppData/Temp directory does not satisfy the directory-mount contract.
$root = Join-Path ([Environment]::GetFolderPath('CommonApplicationData')) ("s3drive session caf" + [char]0xE9 + " ' `$ %USERNAME% $suffix")
$stateRoot = Join-Path $root 'state'
$mount = Join-Path $root 'mounted'
$powershell = Join-Path $env:SystemRoot 'System32\WindowsPowerShell\v1.0\powershell.exe'
$account = $null
$taskName = $null
$taskArguments = [Collections.Generic.List[string]]::new()
$taskNames = @()
$service = New-Object -ComObject Schedule.Service
$service.Connect()
$worker = Join-Path $root 'worker.ps1'
$cleanupFailures = [Collections.Generic.List[string]]::new()
$started = Get-Date
$batchRightGranted = $false
$acceptancePassed = $false
# CI fixture provisioning only. Registration itself still runs as the ordinary
# account. Never grant rights to an existing user or rewrite the host's policy.
# https://learn.microsoft.com/en-us/windows/win32/api/ntsecapi/nf-ntsecapi-lsaaddaccountrights
Add-Type -TypeDefinition @'
using System;
using System.ComponentModel;
using System.Runtime.InteropServices;
using System.Security.Principal;
using System.Text;
public static class S3DriveFixtureNative {
    [StructLayout(LayoutKind.Sequential)]
    struct Luid { public uint Low; public int High; }
    [StructLayout(LayoutKind.Sequential)]
    struct Statistics {
        public Luid TokenId, AuthenticationId;
        public long Expiration;
        public uint Type, ImpersonationLevel, DynamicCharged, DynamicAvailable, GroupCount, PrivilegeCount;
        public Luid ModifiedId;
    }
    [DllImport("advapi32.dll", SetLastError=true)]
    static extern bool OpenProcessToken(IntPtr process, uint access, out IntPtr token);
    [DllImport("advapi32.dll", SetLastError=true)]
    static extern bool GetTokenInformation(IntPtr token, int information, out Statistics value, int length, out int returned);
    [DllImport("kernel32.dll")]
    static extern bool CloseHandle(IntPtr handle);
    [DllImport("userenv.dll", CharSet=CharSet.Unicode)]
    static extern int CreateProfile(string sid, string user, StringBuilder path, uint length);
    [DllImport("userenv.dll", CharSet=CharSet.Unicode, SetLastError=true)]
    static extern bool DeleteProfile(string sid, string path, string computer);
    public static string CreateFixtureProfile(SecurityIdentifier sid, string user) {
        var path = new StringBuilder(260);
        Marshal.ThrowExceptionForHR(CreateProfile(sid.Value, user, path, (uint)path.Capacity));
        return path.ToString();
    }
    public static void DeleteFixtureProfile(SecurityIdentifier sid) {
        if (!DeleteProfile(sid.Value, null, null)) throw new Win32Exception(Marshal.GetLastWin32Error());
    }
    public static string AuthenticationId(IntPtr process) {
        IntPtr token;
        if (!OpenProcessToken(process, 8, out token)) throw new Win32Exception(Marshal.GetLastWin32Error());
        try {
            Statistics value;
            int returned;
            if (!GetTokenInformation(token, 10, out value, Marshal.SizeOf(typeof(Statistics)), out returned))
                throw new Win32Exception(Marshal.GetLastWin32Error());
            return value.AuthenticationId.High.ToString("x8") + ":" + value.AuthenticationId.Low.ToString("x8");
        } finally { CloseHandle(token); }
    }
    [StructLayout(LayoutKind.Sequential)]
    struct Attributes {
        public uint Length;
        public IntPtr RootDirectory, ObjectName;
        public uint Flags;
        public IntPtr SecurityDescriptor, SecurityQualityOfService;
    }
    [StructLayout(LayoutKind.Sequential)]
    struct UnicodeString {
        public ushort Length, MaximumLength;
        public IntPtr Buffer;
    }
    [DllImport("advapi32.dll")]
    static extern uint LsaOpenPolicy(IntPtr system, ref Attributes attributes, uint access, out IntPtr policy);
    [DllImport("advapi32.dll")]
    static extern uint LsaAddAccountRights(IntPtr policy, byte[] sid, UnicodeString[] rights, uint count);
    [DllImport("advapi32.dll")]
    static extern uint LsaRemoveAccountRights(IntPtr policy, byte[] sid, [MarshalAs(UnmanagedType.U1)] bool all, UnicodeString[] rights, uint count);
    [DllImport("advapi32.dll")]
    static extern uint LsaNtStatusToWinError(uint status);
    [DllImport("advapi32.dll")]
    static extern uint LsaClose(IntPtr policy);
    static void Check(uint status) {
        if (status != 0) throw new Win32Exception((int)LsaNtStatusToWinError(status));
    }
    public static void Set(SecurityIdentifier sid, bool grant) {
        byte[] bytes = new byte[sid.BinaryLength];
        sid.GetBinaryForm(bytes, 0);
        var attributes = new Attributes();
        IntPtr policy;
        // POLICY_LOOKUP_NAMES | POLICY_CREATE_ACCOUNT; a new fixture SID has
        // no LSA account-right record yet. This does not create a Windows user.
        Check(LsaOpenPolicy(IntPtr.Zero, ref attributes, 0x810, out policy));
        IntPtr buffer = IntPtr.Zero;
        try {
            const string name = "SeBatchLogonRight";
            buffer = Marshal.StringToHGlobalUni(name);
            var rights = new[] { new UnicodeString {
                Length = (ushort)(name.Length * 2),
                MaximumLength = (ushort)((name.Length + 1) * 2), Buffer = buffer
            }};
            Check(grant ? LsaAddAccountRights(policy, bytes, rights, 1)
                : LsaRemoveAccountRights(policy, bytes, false, rights, 1));
        } finally {
            if (buffer != IntPtr.Zero) Marshal.FreeHGlobal(buffer);
            LsaClose(policy);
        }
    }
}
'@
function User-Action([string]$Operation) {
    $info = [Diagnostics.ProcessStartInfo]::new()
    $info.FileName = $powershell
    $info.Arguments = '-NoLogo -NoProfile -NonInteractive -ExecutionPolicy Bypass -File "' + $worker + '" -Operation ' + $Operation
    $info.UseShellExecute = $false
    $info.CreateNoWindow = $true
    $info.UserName = $userName
    $info.Domain = $env:COMPUTERNAME
    $info.Password = $password
    # The fixture profile is already created. These short-lived clients use
    # files, not HKCU, and must not retain loaded profile references.
    $info.LoadUserProfile = $false
    $info.WorkingDirectory = $root
    $info.RedirectStandardInput = $true
    $info.RedirectStandardOutput = $true
    $info.RedirectStandardError = $true
    $process = [Diagnostics.Process]::Start($info)
    try {
        # CreateProcessWithLogonW can reuse the caller's desktop logon SID;
        # TOKEN_STATISTICS.AuthenticationId identifies the actual LSA logon.
        $authenticationId = [S3DriveFixtureNative]::AuthenticationId($process.Handle)
        $output = $process.StandardOutput.ReadToEndAsync()
        $errors = $process.StandardError.ReadToEndAsync()
        if ($Operation -in @('Install','AuthorityInstall','ManagedInstall')) {
            # The receiving StreamReader recognizes the encoding preamble from
            # Windows PowerShell's console-derived StandardInput writer.
            $process.StandardInput.WriteLine($plainPassword)
        }
        $process.StandardInput.Close()
        if (-not $process.WaitForExit($(if($Operation -eq 'ManagedExercise'){240000}else{150000}))) { $process.Kill(); throw "Timed out in disposable-user $Operation" }
        $text = $output.GetAwaiter().GetResult()
        $errorText = $errors.GetAwaiter().GetResult()
        if ($process.ExitCode -ne 0) { throw "Disposable-user $Operation failed: $errorText $text" }
        if ($Operation -in @('Probe','AuthorityProbe','AuthorityRecover','ManagedProbe')) {
            $probe = $text | ConvertFrom-Json
            $probe | Add-Member -NotePropertyName authentication_id -NotePropertyValue $authenticationId
            return ($probe | ConvertTo-Json -Compress)
        }
        return $text
    } finally { $process.Dispose() }
}
try {
    & wevtutil.exe sl Microsoft-Windows-TaskScheduler/Operational /e:true
    if ($LASTEXITCODE -ne 0) { throw 'Could not enable native task diagnostics' }
    $account = New-LocalUser -Name $userName -Password $password -Description 'Disposable S3Drive native acceptance account'
    if (-not (Get-LocalGroupMember -SID 'S-1-5-32-545' | Where-Object SID -eq $account.SID)) {
        Add-LocalGroupMember -SID 'S-1-5-32-545' -Member $account
    }
    [S3DriveFixtureNative]::Set($account.SID,$true)
    $batchRightGranted = $true
    $profileDirectory = [S3DriveFixtureNative]::CreateFixtureProfile($account.SID,$userName)
    $taskName = "S3Drive-$($account.SID.Value)-acceptance"
    $authorityTaskName = "S3DriveAuthority-$($account.SID.Value)-acceptance"
    $taskNames = @($taskName,$authorityTaskName)
    if ($ManagedFixture) { $taskNames += "S3Drive-$($account.SID.Value)-managed" }
    $null = [IO.Directory]::CreateDirectory($root)
    $acl = [Security.AccessControl.DirectorySecurity]::new()
    $acl.SetOwner($runner.User)
    $acl.SetAccessRuleProtection($true,$false)
    foreach ($allowed in @($runner.User.Value,$account.SID.Value,'S-1-5-18')) {
        $acl.AddAccessRule([Security.AccessControl.FileSystemAccessRule]::new([Security.Principal.SecurityIdentifier]::new($allowed), 'FullControl', 'ContainerInherit,ObjectInherit', 'None', 'Allow'))
    }
    Set-Acl -LiteralPath $root -AclObject $acl
    [IO.File]::WriteAllText((Join-Path $root 'profile-path.txt'), $profileDirectory)
    Copy-Item -LiteralPath $Executable -Destination (Join-Path $root 's3drive.exe')
    if (-not $AuthorityOnly) { Copy-Item -LiteralPath $Config -Destination (Join-Path $root 'drive.toml') }
    if ($AuthorityFixture) {
        Copy-Item -LiteralPath (Join-Path $AuthorityFixture 'cert.pem'),(Join-Path $AuthorityFixture 'key.pem'),(Join-Path $AuthorityFixture 'authority-worker.exe') -Destination $root
        Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'test-windows-authority-worker.ps1') -Destination (Join-Path $root 'authority-worker.ps1')
        Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'test-windows-snapshot-worker.ps1') -Destination (Join-Path $root 'snapshot-worker.ps1')
        Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'test-windows-snapshot-console.cs') -Destination (Join-Path $root 'snapshot-console.cs')
        [IO.File]::WriteAllText((Join-Path $root 'snapshot-fixture.marker'),$account.SID.Value,[Text.UTF8Encoding]::new($false))
    }
    if ($ManagedFixture) {
        foreach ($which in @('first','second')) {
            $null=[IO.Directory]::CreateDirectory((Join-Path $root $which))
            foreach ($file in @('drive.toml','plan.toml','report.toml','policy.toml')) {
                Copy-Item -LiteralPath (Join-Path $ManagedFixture "$which\$file") -Destination (Join-Path $root "$which\$file")
            }
        }
        Copy-Item -LiteralPath (Join-Path $ManagedFixture 'managed-fixture.json') -Destination $root
        [IO.File]::WriteAllText((Join-Path $ManagedFixture 'control-root.txt'),$root,[Text.UTF8Encoding]::new($false))
    }
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
$databaseState=Join-Path $root 'database-state'
$cacheState=Join-Path $root 'cache-state'
# Without loading HKCU, the secondary-logon environment can point USERPROFILE
# at Default. Use the path returned by CreateProfile for fixture file setup;
# the actual scheduled daemon must discover its loaded profile independently.
$profileDirectory=[IO.File]::ReadAllText((Join-Path $root 'profile-path.txt'))
function Manage([string]$Action) {
    $output=& $manager -Action $Action -Name acceptance -StateDirectory $state
    if ($LASTEXITCODE -ne 0) { throw "Manager $Action failed" }
    return $output
}
function Initialize-LocalState([string]$Directory=$databaseState) {
    $output=& (Join-Path $root 's3drive.exe') init-state --directory $Directory | Out-String
    if ($LASTEXITCODE -ne 0 -or $output -notmatch 'schema=39, journal=wal') {
        throw 'Private local state initialization/reopen failed'
    }
}
try {
    if ($Operation.StartsWith('Authority') -or $Operation.StartsWith('Managed')) {
        & (Join-Path $root 'authority-worker.ps1') -Operation $Operation
        exit $LASTEXITCODE
    }
    switch ($Operation) {
        'Install' {
            $reader=[IO.StreamReader]::new([Console]::OpenStandardInput(),[Text.Encoding]::UTF8,$true)
            try { $received=$reader.ReadLine() } finally { $reader.Dispose() }
            if (-not $received.StartsWith('S3d!aA9-') -or $received.Length -ne 40) { throw 'Disposable credential pipe changed the fixture bytes' }
            $secure=ConvertTo-SecureString $received -AsPlainText -Force
            $received=$null
            $identity=[Security.Principal.WindowsIdentity]::GetCurrent()
            $credential=[Management.Automation.PSCredential]::new($identity.Name,$secure)
            $aws=Join-Path $profileDirectory '.aws'
            $null=[IO.Directory]::CreateDirectory($aws)
            [IO.File]::WriteAllText((Join-Path $aws 'credentials'), "[fixture]`naws_access_key_id=TESTPROFILE`naws_secret_access_key=TESTPROFILESECRET`n")
            [IO.File]::WriteAllText((Join-Path $aws 'config'), '')
            Initialize-LocalState
            # Give the scheduled ordinary account its own cache. JSON string
            # escaping is valid for this TOML basic-string filesystem path.
            $cachePath=$cacheState | ConvertTo-Json -Compress
            [IO.File]::AppendAllText((Join-Path $root 'drive.toml'), "`n[cache]`ndirectory=$cachePath`nmax_file_bytes=65536`n")
            & $manager -Action Install -Name acceptance -StateDirectory $state -Executable (Join-Path $root 's3drive.exe') -Config (Join-Path $root 'drive.toml') -MountPoint $mount -Credential $credential
            if ($LASTEXITCODE -ne 0) { throw 'Install failed' }
            $null=Manage Start
        }
        'Probe' {
            # Reopen the same private DB from each independently authenticated
            # ordinary-account process; no loaded profile/HKCU is required.
            Initialize-LocalState
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
            if (-not [IO.File]::Exists((Join-Path $root 'drive.toml')) -or -not [IO.File]::Exists((Join-Path $profileDirectory '.aws\credentials'))) { throw 'Removal deleted config or credentials' }
            if (-not [IO.File]::Exists((Join-Path $cacheState 'metadata.sqlite3'))) { throw 'Mounted cache was not created or removal deleted it' }
            Initialize-LocalState $cacheState
        }
        'CleanupState' {
            # Fixture disposal only: application removal preserves local data.
            # This protected directory grants its owning account and SYSTEM
            # access. Remove it as its owner before deleting that account.
            foreach ($directory in @($databaseState,$cacheState,(Join-Path $root 'authority-state'),(Join-Path $root 'authority-inputs'),(Join-Path $root 'authority-snapshot'),(Join-Path $root 'authority-snapshot-cancel'),(Join-Path $root 'snapshot-alias-output'),(Join-Path $root 'managed-write-state'),(Join-Path $root 'producer-first'),(Join-Path $root 'producer-second'),(Join-Path $root 'bootstrap-200'),(Join-Path $root 'bootstrap-201'),(Join-Path $root 'bootstrap-write'))) {
                if ([IO.Directory]::Exists($directory)) {
                    Remove-Item -LiteralPath $directory -Recurse -Force
                }
                if ([IO.Directory]::Exists($directory)) { throw 'Private database fixture remains after cleanup' }
            }
        }
        'Diagnostics' {
            if ([IO.File]::Exists((Join-Path $root 'authority-worker.ps1'))) {
                & (Join-Path $root 'authority-worker.ps1') -Operation AuthorityDiagnostics
            }
            if ([IO.File]::Exists((Join-Path $state 'startup.json'))) {
                try { Manage Status } catch { Write-Output ("Status diagnostic failed: " + $_.Exception.Message) }
            }
            if ([IO.File]::Exists((Join-Path $state 'session.log'))) { Get-Content -LiteralPath (Join-Path $state 'session.log') -Tail 30 }
        }
    }
} catch { [Console]::Error.WriteLine($_.Exception.Message); exit 1 }
exit 0
'@ | Set-Content -LiteralPath $worker -Encoding UTF8
    if (-not $AuthorityOnly) {
        Write-Output (User-Action Install)
        # The installer process (and its independent logon) has exited. Task
        # Scheduler must keep the mount alive without that process or console.
        $first = User-Action Probe | ConvertFrom-Json
        $second = User-Action Probe | ConvertFrom-Json
        Write-Output ("First login: " + ($first | ConvertTo-Json -Compress))
        Write-Output ("Second login: " + ($second | ConvertTo-Json -Compress))
        if ($first.authentication_id -eq $second.authentication_id -or $first.child_pid -ne $second.child_pid) {
            throw 'Repeated independent logons did not access the same mount'
        }
        $unexpectedRead=$false
        try { $null=[IO.File]::ReadAllText((Join-Path $mount 'other\readme')); $unexpectedRead=$true } catch {}
        if ($unexpectedRead) { throw 'A different account could read the owner-only mount' }
        $unexpectedStateRead=$false
        try { $null=[IO.File]::ReadAllBytes((Join-Path $root 'database-state\metadata.sqlite3')); $unexpectedStateRead=$true } catch {}
        if ($unexpectedStateRead) { throw 'A different account could read the private local database' }
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
    }
    if ($AuthorityFixture) {
        Write-Output (User-Action AuthorityInstall)
        $first = User-Action AuthorityProbe | ConvertFrom-Json
        $second = User-Action AuthorityProbe | ConvertFrom-Json
        if ($first.authentication_id -eq $second.authentication_id -or $first.child_pid -ne $second.child_pid) { throw 'Coordinator not shared across independent logons' }
        Write-Output ('Coordinator first login: ' + ($first | ConvertTo-Json -Compress))
        Write-Output ('Coordinator second login: ' + ($second | ConvertTo-Json -Compress))
        $task=$service.GetFolder('\').GetTask($authorityTaskName)
        if ($task.Xml.Contains($plainPassword)) { throw 'Password leaked into coordinator task' }
        $forbidden=$false
        try { $null=[IO.File]::ReadAllBytes((Join-Path $root 'authority-state\metadata.sqlite3')); $forbidden=$true } catch {}
        if ($forbidden) { throw 'Other account could read coordinator database' }
        $forbidden=$false
        try { $null=[IO.File]::ReadAllBytes((Join-Path $root 'authority-inputs\key.pem')); $forbidden=$true } catch {}
        if ($forbidden) { throw 'Other account could read TLS key' }
        if ($ManagedFixture) {
            Write-Output (User-Action ManagedInstall)
            $managedTask=$service.GetFolder('\').GetTask("S3Drive-$($account.SID.Value)-managed")
            if ($managedTask.Xml.Contains($plainPassword)) { throw 'Password leaked into managed task' }
            $driveFirst=User-Action ManagedProbe | ConvertFrom-Json
            $driveSecond=User-Action ManagedProbe | ConvertFrom-Json
            if ($driveFirst.authentication_id -eq $driveSecond.authentication_id -or $driveFirst.child_pid -ne $driveSecond.child_pid) { throw 'Managed drive not shared across independent logons' }
            Write-Output ('Managed first login: '+($driveFirst | ConvertTo-Json -Compress))
            Write-Output ('Managed second login: '+($driveSecond | ConvertTo-Json -Compress))
            $forbidden=$false
            try { $null=[IO.File]::ReadAllBytes((Join-Path $root ('managed-mounted\empty '+[char]::ConvertFromUtf32(0x1F980)))); $forbidden=$true } catch {}
            if ($forbidden) { throw 'Another account accessed the managed mount' }
            Write-Output (User-Action ManagedExercise)
            Write-Output (User-Action ManagedRemove)
        } else {
            Write-Output (User-Action AuthorityCrash)
            Write-Output (User-Action AuthorityRecover)
        }
        Write-Output (User-Action AuthorityRemove)
        $removed=$false
        try { $null=$service.GetFolder('\').GetTask($authorityTaskName) }
        catch { if ($_.Exception.GetBaseException().HResult -eq -2147024894) { $removed=$true } else { throw } }
        if (-not $removed) { throw 'Coordinator task remains after removal' }
    }
    $acceptancePassed = $true
} catch {
    # Keep the original assertion visible even if cleanup independently fails.
    Write-Output ("Session acceptance failed: " + $_.Exception.Message)
    if ($null -ne $account -and [IO.File]::Exists($worker)) {
        try { Write-Output (User-Action Diagnostics) }
        catch { Write-Warning ("Isolated diagnostics failed: " + $_.Exception.Message) }
    }
    if ($taskName) {
        try {
            Get-WinEvent -FilterHashtable @{LogName='Microsoft-Windows-TaskScheduler/Operational';StartTime=$started} -ErrorAction Stop |
                Where-Object { $message=$_.Message; @($taskNames | Where-Object { $message.Contains($_) }).Count -gt 0 } |
                Select-Object -First 20 Id,Message | Format-List | Out-String | Write-Output
        } catch { Write-Warning 'Task history unavailable' }
    }
    throw
} finally {
    # Cleanup is restricted to the randomly named account, task and directory
    # made here. Force termination is a test-failure cleanup, never app behavior.
    foreach ($ownedTaskName in $taskNames) {
        try {
            $task=$service.GetFolder('\').GetTask($ownedTaskName)
            $taskArguments.Add($task.Definition.Actions.Item(1).Arguments)
            $task.Enabled=$false
            $service.GetFolder('\').DeleteTask($ownedTaskName,0)
        } catch { if ($_.Exception.GetBaseException().HResult -ne -2147024894) { $cleanupFailures.Add('Could not delete disposable task') } }
    }
    if ([IO.Directory]::Exists($root)) {
        foreach ($process in Get-CimInstance Win32_Process -Filter "Name='powershell.exe' OR Name='s3drive.exe'") {
            if (($process.ExecutablePath -eq (Join-Path $root 's3drive.exe')) -or
                ($process.CommandLine -and @($taskArguments | Where-Object { $process.CommandLine.Contains($_) }).Count -gt 0)) {
                Stop-Process -Id $process.ProcessId -Force -ErrorAction SilentlyContinue
                Wait-Process -Id $process.ProcessId -Timeout 15 -ErrorAction SilentlyContinue
            }
        }
    }
    $unmounted=$false
    $mounts=@($mount)
    if ($ManagedFixture) { $mounts += (Join-Path $root 'managed-mounted') }
    $deadline=[DateTime]::UtcNow.AddSeconds(15)
    while ([DateTime]::UtcNow -lt $deadline) {
        $remaining=0
        foreach ($ownedMount in $mounts) {
            try { $null=[IO.File]::GetAttributes($ownedMount); $remaining++ }
            catch {
                if ($_.Exception.GetBaseException() -isnot [IO.FileNotFoundException] -and
                    $_.Exception.GetBaseException() -isnot [IO.DirectoryNotFoundException]) { $remaining++ }
            }
        }
        if ($remaining -eq 0) { $unmounted=$true; break }
        Start-Sleep -Milliseconds 100
    }
    if (-not $unmounted) { $cleanupFailures.Add('Mountpoint still exists; preserving the isolated test directory') }
    if ($null -ne $account) {
        # Windows can start per-user helper processes during a batch logon.
        # All processes owned by this newly created fixture SID belong to this
        # test, even if the OS, rather than our launcher, created them.
        foreach ($process in Get-CimInstance Win32_Process) {
            $owner=Invoke-CimMethod -InputObject $process -MethodName GetOwnerSid -ErrorAction SilentlyContinue
            if ($owner -and $owner.ReturnValue -eq 0 -and $owner.Sid -eq $account.SID.Value) {
                Write-Output ("Stopping disposable-account helper: " + $process.Name + " (" + $process.ProcessId + ")")
                Stop-Process -Id $process.ProcessId -Force -ErrorAction SilentlyContinue
                Wait-Process -Id $process.ProcessId -Timeout 15 -ErrorAction SilentlyContinue
            }
        }
        if ([IO.File]::Exists($worker)) {
            try { $null=User-Action CleanupState }
            catch { $cleanupFailures.Add('Could not remove private database fixture as its owner: ' + $_.Exception.Message) }
        }
        if ($batchRightGranted) {
            try { [S3DriveFixtureNative]::Set($account.SID,$false) }
            catch { $cleanupFailures.Add('Could not remove disposable batch-logon right') }
        }
        try {
            # Ask the supported deletion API rather than treating the WMI
            # Loaded property as proof that deletion must fail. Retry boundedly
            # while the profile service releases its own references.
            $deadline=[DateTime]::UtcNow.AddSeconds(30)
            while ($true) {
                $profile=Get-CimInstance Win32_UserProfile | Where-Object SID -eq $account.SID.Value
                if (-not $profile) { break }
                try { [S3DriveFixtureNative]::DeleteFixtureProfile($account.SID); break }
                catch {
                    if ([DateTime]::UtcNow -gt $deadline) {
                        Write-Output ($profile | Select-Object SID,Loaded,RefCount,Status | ConvertTo-Json -Compress)
                        throw
                    }
                }
                Start-Sleep -Milliseconds 200
            }
            if (Get-CimInstance Win32_UserProfile | Where-Object SID -eq $account.SID.Value) { throw 'Profile registration remains after deletion' }
        } catch {
            $failure=$_.Exception.GetBaseException()
            # Only the explicitly disposable GitHub VM may defer this observed
            # OS-owned hive lock. Every filesystem/security/lifetime assertion
            # must already have passed. Other cleanup errors still fail.
            $disposable=$env:S3DRIVE_DISPOSABLE_WINDOWS_VM -eq '1' -and $env:GITHUB_ACTIONS -eq 'true'
            if ($disposable -and $acceptancePassed -and $unmounted -and
                $failure -is [ComponentModel.Win32Exception] -and $failure.NativeErrorCode -eq 32) {
                foreach ($process in Get-CimInstance Win32_Process) {
                    $owner=Invoke-CimMethod -InputObject $process -MethodName GetOwnerSid -ErrorAction SilentlyContinue
                    if ($owner -and $owner.ReturnValue -eq 0 -and $owner.Sid -eq $account.SID.Value) {
                        $cleanupFailures.Add('Disposable account still owns a process')
                    }
                }
                try {
                    $dummyAws=Join-Path $profileDirectory '.aws'
                    if ([IO.Directory]::Exists($dummyAws)) { Remove-Item -LiteralPath $dummyAws -Recurse -Force }
                }
                catch { $cleanupFailures.Add('Could not remove dummy credentials from retained profile') }
                Write-Warning 'DISPOSABLE VM CLEANUP: Windows retained its locked fixture profile; dummy credentials removed, profile deletion deferred to VM destruction.'
            } else { $cleanupFailures.Add('Could not remove disposable profile: ' + $failure.Message) }
        }
        try { Remove-LocalUser -Name $userName }
        catch { $cleanupFailures.Add('Could not remove disposable user: ' + $_.Exception.Message) }
    }
    if ($unmounted -and [IO.Directory]::Exists($root)) {
        # The private manager state belongs to the disposable user. The elevated
        # runner takes ownership only of this isolated directory during cleanup.
        & takeown.exe /F $root /R /D Y | Out-Null
        if ($LASTEXITCODE -ne 0) { $cleanupFailures.Add('Could not take ownership of disposable test directory') }
        & icacls.exe $root /grant "*$($runner.User.Value):(OI)(CI)F" /T /Q | Out-Null
        if ($LASTEXITCODE -ne 0) { $cleanupFailures.Add('Could not grant cleanup access to disposable test directory') }
        try { Remove-Item -LiteralPath $root -Recurse -Force }
        catch { $cleanupFailures.Add('Could not remove disposable test directory: ' + $_.Exception.Message) }
    }
    $plainPassword=$null
    $password.Dispose()
    if ($cleanupFailures.Count) { throw ($cleanupFailures -join '; ') }
}
if ($ManagedFixture) { Write-Output 'PASS: paired Windows TLS coordinator and managed drive, separate logons, exact unknown recovery, Unicode binary empty data, retained conflict and independent removal' }
if ($AuthorityFixture) { Write-Output 'PASS: non-admin TLS coordinator, independent logons, duplicate starts, supervised crash recovery, exact receipts and controlled removal' }
if (-not $AuthorityOnly) { Write-Output 'PASS: non-admin registration, independent same-account logons, exact S3 and Unicode reads, duplicate starts, retained mount and removal' }
