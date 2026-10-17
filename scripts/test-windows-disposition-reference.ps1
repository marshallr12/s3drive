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
if (@($fixture.records).Count -ne 8 -or @($fixture.records.case | Sort-Object -Unique).Count -ne 8) { throw 'Invalid reference fixture cases' }
foreach ($expected in $fixture.records) {
    $matchingRecords = @($records | Where-Object { $_.case -ceq $expected.case })
    if ($matchingRecords.Count -ne 1 -or $matchingRecords[0].backend -cne $expected.backend) { throw "Reference case missing or changed: $($expected.case)" }
    $actual = $matchingRecords[0].observations
    $keys = @($expected.observations.PSObject.Properties.Name)
    if (@($actual.PSObject.Properties).Count -ne $keys.Count) { throw "Reference fields changed: $($expected.case)" }
    foreach ($key in $keys) {
        $actualValue = ConvertTo-Json -InputObject $actual.$key -Compress
        $expectedValue = ConvertTo-Json -InputObject $expected.observations.$key -Compress
        if ($actualValue -cne $expectedValue) { throw "Reference observation changed: $($expected.case).$key" }
    }
}
Write-Output 'PASS: all eight previously recorded NTFS traces match'
# Measurement phase only: new values are deliberately not guessed. The final
# acceptance revision must freeze all fourteen records and remove this allowance.
$measuredCases = @('deferred-file-rename', 'deferred-directory-rename', 'created-deferred-file', 'multiple-deferred-cancel', 'extended-on-close-cancel', 'duplicated-deferred-handle')
$caseNames = @($fixture.records.case) + $measuredCases
foreach ($caseName in $caseNames) {
    if (@($records | Where-Object { $_.case -ceq $caseName -and $_.backend -ceq 'ntfs' }).Count -ne 1) { throw "Missing measured case: $caseName" }
}
Write-Output 'MEASURED: six additional NTFS traces require review and a frozen fixture before publication'
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
        operating_system = (Get-CimInstance Win32_OperatingSystem).Caption
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
