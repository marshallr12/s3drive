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
if ($referenceExit -ne 0 -or $referenceText -notmatch '8 passed; 0 failed' -or $records.Count -ne 8 -or @($records.case | Sort-Object -Unique).Count -ne 8) {
    throw "NTFS disposition reference failed or omitted cases: $referenceExit"
}
$root = Join-Path $env:RUNNER_TEMP ('s3drive-disposition-reference-' + [Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $root | Out-Null
$passed = $false
try {
    # This source-pinned test covers a nonempty delete-on-close directory on
    # WinFsp's own disk and network memfs backends. It is reference evidence,
    # separate from S3Drive's seven real SDK-backed native acceptance cases.
    $listing = & $env:S3DRIVE_WINFSP_TEST_EXE --legacy-unlink-rename --list +create_test
    if ($LASTEXITCODE -ne 0 -or ($listing -join "`n") -notmatch '\bcreate_test\b') { throw 'Pinned create_test selector missing' }
    $output = Join-Path $root 'stdout.txt'
    $errors = Join-Path $root 'stderr.txt'
    $process = Start-Process -FilePath $env:S3DRIVE_WINFSP_TEST_EXE -ArgumentList @('--legacy-unlink-rename', '+create_test') -WorkingDirectory $root -RedirectStandardOutput $output -RedirectStandardError $errors -PassThru
    if (-not $process.WaitForExit(60000)) {
        $process.Kill()
        throw 'Pinned WinFsp disposition reference timed out'
    }
    $process.Refresh()
    $text = Get-Content -Raw -LiteralPath $output
    Write-Output $text
    if (Test-Path -LiteralPath $errors) { Get-Content -LiteralPath $errors | Write-Output }
    if ($process.ExitCode -ne 0 -or $text -notmatch 'create_test\.+\s+OK') {
        throw "Pinned WinFsp disposition reference failed: $($process.ExitCode)"
    }
    $passed = $true
    Write-Output 'PASS: eight NTFS reference probes and pinned WinFsp legacy disk/network create_test; not S3Drive deletion acceptance'
} finally {
    if ($passed) { Remove-Item -LiteralPath $root -Recurse -Force }
    else { Write-Output "Retained reference fixture diagnostics: $root" }
}
