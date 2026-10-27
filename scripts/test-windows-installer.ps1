param(
    [Parameter(Mandatory=$true)][string]$Current,
    [Parameter(Mandatory=$true)][string]$Previous,
    [Parameter(Mandatory=$true)][string]$Rollback
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
if ($env:GITHUB_ACTIONS -ne 'true' -or $env:RUNNER_OS -ne 'Windows') {
    throw 'Installer acceptance is restricted to disposable GitHub-hosted Windows runners.'
}
$currentInfo = Get-Content (Join-Path $Current 'installer.json') -Raw | ConvertFrom-Json
$previousInfo = Get-Content (Join-Path $Previous 'installer.json') -Raw | ConvertFrom-Json
$currentMsi = Join-Path $Current $currentInfo.msi
$previousMsi = Join-Path $Previous $previousInfo.msi
$rollbackInfo = Get-Content (Join-Path $Rollback 'installer.json') -Raw | ConvertFrom-Json
$rollbackMsi = Join-Path $Rollback $rollbackInfo.msi
$root = Join-Path $env:RUNNER_TEMP ('s3drive-installer-test-' + [guid]::NewGuid().ToString('N'))
$install = Join-Path $env:ProgramFiles 'S3Drive'
if (Test-Path $install) { throw 'Refusing preexisting S3Drive install directory.' }
New-Item -ItemType Directory $root | Out-Null
$sentinel = Join-Path $root 'user data 日本語'
New-Item -ItemType Directory $sentinel | Out-Null
$config = Join-Path $sentinel 's3drive.local.toml'
[IO.File]::WriteAllText($config, "version=1`nmode='read-only'`n[[sources]]`nalias='docs'`nbucket='fixture'`nregion='us-east-1'`nendpoint='http://127.0.0.1:1'`nallow_http_loopback=true`npath_style=true`n")
$dirty = Join-Path $sentinel 'unsaved-user-data.bin'
[IO.File]::WriteAllBytes($dirty, [byte[]](0..255))
$configHash = (Get-FileHash $config).Hash
$dirtyHash = (Get-FileHash $dirty).Hash
$manual = Join-Path $install 'user-created-do-not-delete.txt'
$binary = Join-Path $install 's3drive.exe'
$sequence = [Collections.Generic.List[string]]::new()
function Msi([string]$Label, [string[]]$Arguments, [int[]]$Expected = @(0), [string]$Message = '') {
    $log = Join-Path $root ($Label + '.log')
    $process = Start-Process msiexec.exe -ArgumentList ($Arguments + @('/qn', '/norestart', 'REBOOT=ReallySuppress', '/l*v', "`"$log`"")) -Wait -PassThru
    if ($Expected -notcontains $process.ExitCode) {
        Select-String -Path $log -Pattern 'error|Return value 3|FailWhenDeferred|ScriptType=2|RemoveExistingProducts' | Select-Object -Last 40 | ForEach-Object { Write-Output $_.Line }
        throw "$Label exit $($process.ExitCode), expected $Expected; log $log" }
    if ($Message -and -not (Select-String -Path $log -SimpleMatch $Message -Quiet)) {
        Select-String -Path $log -Pattern 'error|Return value 3|FailWhenDeferred|ScriptType=2|RemoveExistingProducts' | Select-Object -Last 40 | ForEach-Object { Write-Output $_.Line }
        throw "$Label lacked required refusal reason"
    }
    $script:LastMsiLog = $log
    $sequence.Add("$Label=$($process.ExitCode)")
}
function Assert-UserData {
    if ((Get-FileHash $config).Hash -ne $configHash -or (Get-FileHash $dirty).Hash -ne $dirtyHash) { throw 'User config/dirty bytes changed.' }
}
function Assert-Payload($Info = $currentInfo) {
    $manifest = Get-Content (Join-Path $install 'build-manifest.json') -Raw | ConvertFrom-Json
    if ($manifest.source_commit -ne $Info.source -or $manifest.msi_version -ne $Info.version) { throw 'Installed source mismatch.' }
    foreach ($file in $manifest.files.PSObject.Properties) {
        $path = Join-Path $install $file.Name
        if ((Get-FileHash $path -Algorithm SHA256).Hash.ToLowerInvariant() -ne $file.Value) { throw "Installed payload mismatch: $($file.Name)" }
    }
    & $binary --version
    if ($LASTEXITCODE -ne 0) { throw 'Installed CLI version failed.' }
    & $binary config-check $config
    if ($LASTEXITCODE -ne 0) { throw 'Installed CLI config-check failed.' }
    Assert-UserData
}
$winfspMsi = Join-Path $env:RUNNER_TEMP 'winfsp-2.1.25156.msi'
if (-not (Test-Path $winfspMsi)) { throw 'Pinned CI-only WinFsp MSI absent.' }
try {
    # Remove only the dependency installed earlier on this disposable VM. Never a user machine.
    Msi 'remove-test-driver' @('/x', "`"$winfspMsi`"")
    Msi 'missing-dependency' @('/i', "`"$currentMsi`"") @(1603) 'Install the official WinFsp 2.1 x64 runtime first'
    if (Test-Path $install) { throw 'Failed prerequisite left application payload.' }
    Assert-UserData
    Msi 'restore-test-driver' @('/i', "`"$winfspMsi`"", 'ADDLOCAL=ALL')
    Msi 'install-previous' @('/i', "`"$previousMsi`"")
    [IO.File]::WriteAllText($manual, 'user-owned extra file; installer must preserve this')
    $manualHash = (Get-FileHash $manual).Hash
    Assert-UserData
    # Exercise the installed binary's real durable state path, not a mock binary.
    $state = Join-Path $sentinel 'private-state'
    & $binary init-state --directory $state
    if ($LASTEXITCODE -ne 0) { throw 'Installed CLI durable init failed.' }
    $db = Join-Path $state 'metadata.sqlite3'
    if (-not (Test-Path $db)) { throw 'Installed CLI database absent.' }
    $dbHash = (Get-FileHash $db).Hash
    Assert-Payload $previousInfo
    Msi 'failed-upgrade-rollback' @('/i', "`"$rollbackMsi`"", 'WIXFAILWHENDEFERRED=1') @(1603) 'FailWhenDeferred'
    python scripts/check-msi-rollback.py $script:LastMsiLog
    if ($LASTEXITCODE -ne 0) { throw 'MSI deferred failure/rollback sequence was not proven.' }
    Assert-Payload $previousInfo
    if ((Get-FileHash $manual).Hash -ne $manualHash -or (Get-FileHash $db).Hash -ne $dbHash) { throw 'Failed upgrade rollback changed user data.' }
    $oldRegistration = "HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\$($previousInfo.product_code)"
    if (-not (Test-Path $oldRegistration)) { throw 'Rollback did not restore old product registration.' }
    Msi 'upgrade-current' @('/i', "`"$currentMsi`"")
    Assert-Payload
    if ((Get-FileHash $manual).Hash -ne $manualHash) { throw 'Upgrade removed user-created file.' }
    if ((Get-FileHash $db).Hash -ne $dbHash) { throw 'Upgrade changed user database.' }
    # Existing exact native test now launches the MSI-installed executable.
    $oldPackaged = $env:S3DRIVE_TEST_PACKAGED_CLI
    try {
        $env:S3DRIVE_TEST_PACKAGED_CLI = $binary
        cargo test -p s3drive-cli --test chunk_publication --release hosted_application_git_save_replace_rename_delete_and_interrupted_cold_remount --locked -- --exact --ignored --nocapture --test-threads=1
        if ($LASTEXITCODE -ne 0) { throw 'MSI-installed CLI native application fixture failed.' }
        $sequence.Add('installed-native-application=passed')
    } finally {
        $env:S3DRIVE_TEST_PACKAGED_CLI = $oldPackaged
    }
    Msi 'repair-current'  @('/fa', "`"$currentMsi`"")
    Assert-Payload
    Msi 'repeat-current' @('/i', "`"$currentMsi`"")
    Assert-Payload
    Msi 'refuse-downgrade' @('/i', "`"$previousMsi`"") @(1603) 'A newer S3Drive version is installed'
    Assert-Payload
    if ((Get-FileHash $db).Hash -ne $dbHash) { throw 'Repair/repeat/downgrade changed user database.' }
    # Uninstall must work even if the independently managed runtime has gone away.
    Msi 'remove-test-driver-again' @('/x', "`"$winfspMsi`"")
    Msi 'uninstall-without-driver' @('/x', "`"$currentMsi`"")
    if (Test-Path $binary) { throw 'Uninstall retained owned executable.' }
    if ((Get-FileHash $manual).Hash -ne $manualHash -or (Get-FileHash $db).Hash -ne $dbHash) { throw 'Uninstall changed user-created file/database.' }
    Assert-UserData
    # Inspect registered products without Win32_Product (which would trigger repairs).
    $registration = "HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\$($currentInfo.product_code)"
    if (Test-Path $registration) { throw 'Uninstall retained MSI registration.' }
    Msi 'reinstall-current' @('/i', "`"$winfspMsi`"", 'ADDLOCAL=ALL')
    Msi 'reinstall-product' @('/i', "`"$currentMsi`"")
    Assert-Payload
    Msi 'final-uninstall' @('/x', "`"$currentMsi`"")
    Assert-UserData
    if ((Get-FileHash $manual).Hash -ne $manualHash -or (Get-FileHash $db).Hash -ne $dbHash) { throw 'Final uninstall changed user data.' }
    # Only this exact fixture-owned sentinel is removed; no recursive product/data cleanup.
    Remove-Item -LiteralPath $manual
    if (@(Get-ChildItem -LiteralPath $install -Force).Count -ne 0) { throw 'Unexpected installed leftovers.' }
    Remove-Item -LiteralPath $install
    Write-Output ('WINDOWS_INSTALLER_ACCEPTANCE ' + (@{source=$currentInfo.source; msi_sha256=$currentInfo.sha256; previous_version=$previousInfo.version; current_version=$currentInfo.version; checks=$sequence.ToArray(); user_data='unchanged'; public_release=$false; gui_tested=$false; reboot_tested=$false} | ConvertTo-Json -Compress))
} catch {
    # Retain failure state and logs on the disposable runner for diagnosis; do not hide a failure.
    Write-Output "Installer fixture failed; retained diagnostics: $root"
    throw
}
