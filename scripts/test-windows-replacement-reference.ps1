param([switch]$Measure)
$ErrorActionPreference='Stop'
Set-StrictMode -Version Latest
if(-not $env:RUNNER_TEMP -or -not $env:GITHUB_SHA -or -not $env:S3DRIVE_WINFSP_TEST_EXE) {throw 'Use the prepared disposable Windows runner'}
$output=& cargo test -p s3drive-winfsp --locked replacement_reference -- --ignored --nocapture --test-threads=1 2>&1
$exit=$LASTEXITCODE
$output|Write-Output
$records=@(($output -join "`n") -split "`n"|ForEach-Object {if($_ -match 'S3DRIVE_REPLACEMENT_REFERENCE (\{.*\})') {$Matches[1]|ConvertFrom-Json}})
if($exit -ne 0 -or ($output -join "`n") -notmatch '1 passed; 0 failed' -or $records.Count -ne 21) {throw "Replacement reference omitted or failed cases: $exit"}
$sha=(& git rev-parse HEAD).Trim()
if($LASTEXITCODE -ne 0 -or $sha -cne $env:GITHUB_SHA) {throw 'Replacement source does not match CI commit'}
$directory=(Get-ItemProperty 'HKLM:\SOFTWARE\WOW6432Node\WinFsp').InstallDir
$observed=[ordered]@{
    source_commit=$sha
    operating_system=(Get-CimInstance Win32_OperatingSystem).Caption
    winfsp_file_version=(Get-Item (Join-Path $directory 'bin\winfsp-x64.dll')).VersionInfo.FileVersion
    observed_at_utc=(Get-Date).ToUniversalTime().ToString('yyyy-MM-ddTHH:mm:ssZ')
    records=$records
}
. (Join-Path $PSScriptRoot 'windows-replacement-fixture.ps1')
if($Measure) {
    # Measurement validates the full matrix and captures observations. It does
    # not freeze guessed outcomes or count as parity/native acceptance.
    Assert-WindowsReplacementReference -Fixture ([pscustomobject]$observed) -Observation ([pscustomobject]$observed)
    Write-Output 'MEASURED: seven cases on NTFS and selected WinFsp legacy/POSIX profiles; review before publication'
} else {
    $fixture=Get-Content -Raw -LiteralPath (Join-Path $PSScriptRoot '..\docs\src\evidence\windows-replacement-reference-20261017.json')|ConvertFrom-Json
    Assert-WindowsReplacementReference -Fixture $fixture -Observation ([pscustomobject]$observed)
    Write-Output 'PASS: twenty-one typed replacement observations match the recorded platform'
}
$observed|ConvertTo-Json -Depth 8|Set-Content -LiteralPath (Join-Path $env:RUNNER_TEMP 'windows-replacement-reference.json') -Encoding utf8
