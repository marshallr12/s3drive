#requires -Version 5.1
# Only the newly created ordinary account in test-windows-session.ps1.
# No real user state, service installation, provider access or restoration.
param([Parameter(Mandatory)][ValidateSet('Active','Stopped','Cancel')][string]$Phase)
$ErrorActionPreference='Stop'
Set-StrictMode -Version Latest
$root=$PSScriptRoot
$identity=[Security.Principal.WindowsIdentity]::GetCurrent()
if (([Security.Principal.WindowsPrincipal]::new($identity)).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) { throw 'Snapshot fixture must use the ordinary account' }
$marker=Join-Path $root 'snapshot-fixture.marker'
if (-not [IO.File]::Exists($marker) -or [IO.File]::ReadAllText($marker) -cne $identity.User.Value) { throw 'Not an owned disposable snapshot fixture' }
$exe=Join-Path $root 's3drive.exe'
$db=Join-Path $root 'authority-state'
$snapshot=Join-Path $root 'authority-snapshot'
$pin=Get-Content -LiteralPath (Join-Path $root 'authority-identity.json') -Raw -Encoding UTF8 | ConvertFrom-Json
function Quote([string]$Text) {
    if ($Text.Contains('"') -or $Text.Contains([char]0)) { throw 'Invalid fixture argument' }
    return '"'+[regex]::Replace($Text,'(\\+)$','$1$1')+'"'
}
function Invoke-Snapshot([string]$Verb,[string]$Directory,[string]$Output='') {
    $arguments=@('authority-snapshot',$Verb,'--directory',$Directory,'--expect-volume',$pin.volume,'--name-policy','portable-collision-v1')
    if ($Output) { $arguments+=@('--output',$Output) }
    $info=[Diagnostics.ProcessStartInfo]::new()
    $info.FileName=$exe
    $info.Arguments=($arguments | ForEach-Object { Quote $_ }) -join ' '
    $info.UseShellExecute=$false
    $info.CreateNoWindow=$true
    $info.RedirectStandardInput=$true
    $info.RedirectStandardOutput=$true
    $info.RedirectStandardError=$true
    $info.EnvironmentVariables['AWS_CONFIG_FILE']=Join-Path $root 'absent-config'
    $info.EnvironmentVariables['AWS_SHARED_CREDENTIALS_FILE']=Join-Path $root 'absent-credentials'
    $info.EnvironmentVariables['AWS_EC2_METADATA_DISABLED']='true'
    $child=[Diagnostics.Process]::Start($info)
    try {
        $child.StandardInput.Close()
        $stdout=$child.StandardOutput.ReadToEndAsync()
        $stderr=$child.StandardError.ReadToEndAsync()
        if (-not $child.WaitForExit(20000)) { $child.Kill();$child.WaitForExit();throw 'Snapshot fixture child deadline' }
        return [pscustomobject]@{Code=$child.ExitCode;Text=$stdout.GetAwaiter().GetResult();Error=$stderr.GetAwaiter().GetResult()}
    } finally { $child.Dispose() }
}
function Refused($Result) {
    if ($Result.Code -eq 0 -or $Result.Text.Length -ne 0) { throw 'Invalid snapshot request reported success' }
}
function Parsed($Result) {
    if ($Result.Code -ne 0) { throw ('Snapshot fixture failed: '+$Result.Error) }
    $value=$Result.Text | ConvertFrom-Json
    if ($value.history_continuity_proven -isnot [bool] -or $value.history_continuity_proven -or
        $value.restore_authorized -isnot [bool] -or $value.restore_authorized -or
        $value.manifest.volume.id -cne $pin.volume -or
        $value.manifest.volume.name_policy -cne 'portable-collision-v1' -or
        $value.manifest.format -cne 's3drive-authority-snapshot' -or
        $value.manifest.schema_version -ne 38) { throw 'Snapshot identity/validation contract changed' }
    return $value
}
if ($Phase -eq 'Cancel') {
    # This actor is Windows PowerShell5.1; compile a test-only .NET console
    # helper here, not inside the product or the runner's shared console.
    $helper=Join-Path $root 'snapshot-console.exe'
    if ([IO.File]::Exists($helper)) { throw 'Console helper already exists' }
    Add-Type -Path (Join-Path $root 'snapshot-console.cs') -OutputAssembly $helper -OutputType ConsoleApplication
    $info=[Diagnostics.ProcessStartInfo]::new()
    $info.FileName=$helper
    $info.Arguments=(Quote $root)+' '+(Quote $pin.volume)
    $info.UseShellExecute=$false
    $info.CreateNoWindow=$true
    $info.RedirectStandardInput=$true
    $info.RedirectStandardOutput=$true
    $info.RedirectStandardError=$true
    $child=[Diagnostics.Process]::Start($info)
    try {
        $child.StandardInput.Close()
        $stdout=$child.StandardOutput.ReadToEndAsync()
        $stderr=$child.StandardError.ReadToEndAsync()
        if (-not $child.WaitForExit(90000)) { $child.Kill();$child.WaitForExit();throw 'Private-console helper deadline; fixture must fail and reap owned account processes' }
        $text=$stdout.GetAwaiter().GetResult()
        $errorText=$stderr.GetAwaiter().GetResult()
        $result=Get-Content -LiteralPath (Join-Path $root 'snapshot-console-result.json') -Raw -Encoding UTF8 | ConvertFrom-Json
        if ($child.ExitCode -ne 0) { throw ('Private-console snapshot fixture failed: '+($result|ConvertTo-Json -Compress)) }
        if ($text.Length -ne 0 -or $errorText.Length -ne 0) { throw 'Unexpected helper output' }
        foreach($field in @('private_console_verified','cooperative_cancel_observed','completion_manifest_absent','source_sha256_preserved','partial_inspection_refused','same_destination_retry_refused')) {
            if ($result.$field -isnot [bool] -or -not $result.$field) { throw 'Missing positive console assertion' }
        }
        foreach($field in @('success_stdout','restore_authorized')) {
            if ($result.$field -isnot [bool] -or $result.$field) { throw 'Unsafe console result' }
        }
        if (($result.cancel_to_exit_ms -isnot [int] -and $result.cancel_to_exit_ms -isnot [long]) -or $result.cancel_to_exit_ms -lt 0 -or $result.cancel_to_exit_ms -gt 20000) { throw 'Invalid cancellation observation' }
        Write-Output ('PASS: ordinary-account Windows private-console Ctrl+C, source bytes/partial output preserved, no completion or replacement: '+($result|ConvertTo-Json -Compress))
    } finally { $child.Dispose() }
    exit 0
}
if ($Phase -eq 'Active') {
    Refused (Invoke-Snapshot create $db $snapshot)
    if ([IO.Directory]::Exists($snapshot)) { throw 'Active owner refusal created output' }
    Write-Output 'PASS: ordinary-account snapshot refuses the live coordinator'
    exit 0
}
$sourceBefore=(Get-FileHash -LiteralPath (Join-Path $db 'metadata.sqlite3') -Algorithm SHA256).Hash
$created=Parsed (Invoke-Snapshot create $db $snapshot)
$inspected=Parsed (Invoke-Snapshot inspect $snapshot)
if (($created|ConvertTo-Json -Depth 10 -Compress) -cne ($inspected|ConvertTo-Json -Depth 10 -Compress)) { throw 'Snapshot inspection changed its manifest' }
$database=Join-Path $snapshot 'snapshot.sqlite3'
$manifest=Join-Path $snapshot 'snapshot.json'
$before=(Get-FileHash -LiteralPath $database -Algorithm SHA256).Hash
$markerBefore=(Get-FileHash -LiteralPath $manifest -Algorithm SHA256).Hash
Refused (Invoke-Snapshot create $db $snapshot)
if ((Get-FileHash -LiteralPath $database -Algorithm SHA256).Hash -cne $before -or
    (Get-FileHash -LiteralPath $manifest -Algorithm SHA256).Hash -cne $markerBefore) { throw 'Existing artifact was replaced' }
# A readable existing handle must coexist with inspector's no-delete sharing.
$handle=[IO.File]::Open($database,'Open','Read','ReadWrite')
try { $null=Parsed (Invoke-Snapshot inspect $snapshot) } finally { $handle.Dispose() }
# Change only this owned fixture ACL, then restore the exact descriptor.
$saved=Get-Acl -LiteralPath $database
$changed=[Security.AccessControl.FileSecurity]::new()
$changed.SetSecurityDescriptorBinaryForm($saved.GetSecurityDescriptorBinaryForm())
$changed.AddAccessRule([Security.AccessControl.FileSystemAccessRule]::new([Security.Principal.SecurityIdentifier]::new('S-1-1-0'),'Read','Allow'))
try {
    Set-Acl -LiteralPath $database -AclObject $changed
    Refused (Invoke-Snapshot inspect $snapshot)
} finally { Set-Acl -LiteralPath $database -AclObject $saved }
$null=Parsed (Invoke-Snapshot inspect $snapshot)
$hardlink=Join-Path $root 'snapshot-hardlink'
try {
    $null=New-Item -ItemType HardLink -Path $hardlink -Value $database
    Refused (Invoke-Snapshot inspect $snapshot)
} finally { if ([IO.File]::Exists($hardlink)) { [IO.File]::Delete($hardlink) } }
$null=Parsed (Invoke-Snapshot inspect $snapshot)
$junction=Join-Path $root 'snapshot-source-junction'
$aliasOutput=Join-Path $root 'snapshot-alias-output'
try {
    $null=New-Item -ItemType Junction -Path $junction -Value $db
    Refused (Invoke-Snapshot create $junction $aliasOutput)
    if ([IO.Directory]::Exists($aliasOutput)) { throw 'Reparse source created output' }
} finally { if ([IO.Directory]::Exists($junction)) { [IO.Directory]::Delete($junction) } }
$nested=Join-Path ($db.ToUpperInvariant()) 'snapshot-nested'
Refused (Invoke-Snapshot create $db $nested)
if ([IO.Directory]::Exists($nested)) { throw 'Case alias nested artifact was created' }
if ((Get-FileHash -LiteralPath (Join-Path $db 'metadata.sqlite3') -Algorithm SHA256).Hash -cne $sourceBefore) { throw 'Offline snapshot checks modified source database' }
if ([IO.File]::Exists((Join-Path $snapshot 'metadata.sqlite3')) -or [IO.File]::Exists((Join-Path $snapshot 'volume.lock'))) { throw 'Snapshot acquired live authority filenames' }
Write-Output 'PASS: ordinary-account Windows offline snapshot create/inspect, exact source bytes, no replacement/restore, private ACL, hardlink/junction/case-alias refusal and read-handle sharing'
