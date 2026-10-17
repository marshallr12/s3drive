# Explicit disposable CI acceptance. No S3Drive deletion callback is enabled.
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
if (-not $env:RUNNER_TEMP -or -not $env:S3DRIVE_WINFSP_TEST_EXE) {
    throw 'Run on the prepared disposable Windows runner with pinned WinFsp tests'
}
$reference = & cargo test -p s3drive-winfsp --locked disposition_reference -- --ignored --nocapture --test-threads=1 2>&1
$referenceExit = $LASTEXITCODE
$reference | Write-Output
$referenceText = $reference -join "`n"
$records = @($referenceText -split "`n" | ForEach-Object {
    if ($_ -match 'S3DRIVE_DISPOSITION_REFERENCE (\{.*\})') { $Matches[1] | ConvertFrom-Json }
})
if ($referenceExit -ne 0 -or $referenceText -notmatch '14 passed; 0 failed' -or $records.Count -ne 14 -or @($records.case | Sort-Object -Unique).Count -ne 14) {
    throw "NTFS disposition reference failed or omitted cases: $referenceExit"
}
$fixture = Get-Content -Raw -LiteralPath (Join-Path $PSScriptRoot '..\docs\src\evidence\windows-disposition-reference-20261017.json') | ConvertFrom-Json
. (Join-Path $PSScriptRoot 'windows-reference-fixture.ps1')
$observations = [pscustomobject]@{ operating_system = (Get-CimInstance Win32_OperatingSystem).Caption; records = $records }
Assert-WindowsDispositionReference -Fixture $fixture -Observation $observations
Write-Output 'PASS: all fourteen NTFS traces match the recorded Windows Server 2025 reference'
$root = Join-Path $env:RUNNER_TEMP ('s3drive-disposition-reference-' + [Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $root | Out-Null
$passed = $false
try {
    # These pinned upstream cases use their own memfs disk/network backends.
    # They are not the S3Drive native acceptance cases or NTFS parity proof.
    $upstreamCases = @('create_test', 'delete_pending_test', 'rename_test', 'rename_open_test')
    foreach ($caseName in $upstreamCases) {
        $selector = '+' + $caseName
        $listing = & $env:S3DRIVE_WINFSP_TEST_EXE --legacy-unlink-rename --list $selector
        if ($LASTEXITCODE -ne 0 -or ($listing -join "`n") -notmatch ('\b' + [regex]::Escape($caseName) + '\b')) { throw "Pinned selector missing: $caseName" }
        $output = Join-Path $root ($caseName + '.stdout.txt')
        $errors = Join-Path $root ($caseName + '.stderr.txt')
        $process = Start-Process -FilePath $env:S3DRIVE_WINFSP_TEST_EXE -ArgumentList @('--legacy-unlink-rename', $selector) -WorkingDirectory $root -RedirectStandardOutput $output -RedirectStandardError $errors -PassThru
        if (-not $process.WaitForExit(60000)) {
            $process.Kill()
            throw "Pinned reference timed out: $caseName"
        }
        $process.Refresh()
        $text = Get-Content -Raw -LiteralPath $output
        Write-Output $text
        if (Test-Path -LiteralPath $errors) { Get-Content -LiteralPath $errors | Write-Output }
        if ($process.ExitCode -ne 0 -or $text -notmatch ([regex]::Escape($caseName) + '\.+\s+OK')) { throw "Pinned reference failed: $caseName ($($process.ExitCode))" }
    }
    $referenceSha = (& git rev-parse HEAD).Trim()
    if ($LASTEXITCODE -ne 0 -or $referenceSha -notmatch '^[0-9a-f]{40}$' -or $referenceSha -cne $env:GITHUB_SHA) { throw 'Reference evidence source does not match this CI commit' }
    $evidence = [ordered]@{
        source_commit = $referenceSha
        operating_system = $observations.operating_system
        observed_at_utc = (Get-Date).ToUniversalTime().ToString('yyyy-MM-ddTHH:mm:ssZ')
        records = $records
        upstream_legacy_cases = $upstreamCases
    }
    $evidence | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath (Join-Path $env:RUNNER_TEMP 'windows-disposition-reference.json') -Encoding utf8
    $passed = $true
    Write-Output 'PASS: fourteen NTFS reference probes and four pinned WinFsp legacy disk/network cases; not S3Drive deletion acceptance'
} finally {
    if ($passed) { Remove-Item -LiteralPath $root -Recurse -Force }
    else { Write-Output "Retained reference fixture diagnostics: $root" }
}
