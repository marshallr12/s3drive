# Runs on PowerShell without Windows native APIs. Test the oracle's failure paths.
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
. (Join-Path $PSScriptRoot 'windows-reference-fixture.ps1')
$fixture = Get-Content -Raw -LiteralPath (Join-Path $PSScriptRoot '..\docs\src\evidence\windows-disposition-reference-20261017.json') | ConvertFrom-Json
$good = [pscustomobject]@{ operating_system = $fixture.operating_system; records = $fixture.records }
$raw = $good | ConvertTo-Json -Depth 8
Assert-WindowsDispositionReference -Fixture $fixture -Observation ($raw | ConvertFrom-Json)
$cases = @(
    @{ name = 'missing case'; change = { param($o) $o.records = @($o.records | Select-Object -SkipLast 1) } },
    @{ name = 'duplicate case'; change = { param($o) $o.records[13] = $o.records[0] } },
    @{ name = 'unknown case'; change = { param($o) $o.records[0].case = 'unreviewed-case' } },
    @{ name = 'wrong backend'; change = { param($o) $o.records[0].backend = 'another-filesystem' } },
    @{ name = 'wrong OS'; change = { param($o) $o.operating_system = 'unreviewed Windows version' } },
    @{ name = 'extra field'; change = { param($o) $o.records[0].observations | Add-Member -NotePropertyName 'extra' -NotePropertyValue $null } },
    @{ name = 'changed null'; change = { param($o) ($o.records | Where-Object case -CEQ 'extended-on-close-cancel').observations.cancel_hresult = 0 } },
    @{ name = 'changed boolean'; change = { param($o) ($o.records | Where-Object case -CEQ 'deferred-file-open').observations.pending_before_cleanup = $true } },
    @{ name = 'renamed null field'; change = { param($o)
        $fields = ($o.records | Where-Object case -CEQ 'extended-on-close-cancel').observations
        $fields.PSObject.Properties.Remove('cancel_hresult')
        $fields | Add-Member -NotePropertyName 'different_null' -NotePropertyValue $null
    } }
)
foreach ($case in $cases) {
    $actual = $raw | ConvertFrom-Json
    & $case.change $actual
    $rejected = $false
    try { Assert-WindowsDispositionReference -Fixture $fixture -Observation $actual }
    catch { $rejected = $true }
    if (-not $rejected) { throw "Reference checker accepted $($case.name)" }
}
Write-Output 'PASS: fourteen-case fixture and nine malformed-observation rejections'
