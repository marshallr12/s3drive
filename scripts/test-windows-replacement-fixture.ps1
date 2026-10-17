# Pure typed-oracle checks; no native handles, providers or credentials.
$ErrorActionPreference='Stop'
Set-StrictMode -Version Latest
. (Join-Path $PSScriptRoot 'windows-replacement-fixture.ps1')
$fixture=Get-Content -Raw -LiteralPath (Join-Path $PSScriptRoot '..\docs\src\evidence\windows-replacement-reference-20261017.json')|ConvertFrom-Json
$raw=([pscustomobject]@{operating_system=$fixture.operating_system;winfsp_file_version=$fixture.winfsp_file_version;records=$fixture.records})|ConvertTo-Json -Depth 8
Assert-WindowsReplacementReference -Fixture $fixture -Observation ($raw|ConvertFrom-Json)
$cases=@(
 @{name='missing record';change={param($o)$o.records=@($o.records|Select-Object -SkipLast 1)}},
 @{name='duplicate';change={param($o)$o.records[20]=$o.records[0]}},
 @{name='unknown case';change={param($o)$o.records[0].case='not-reviewed'}},
 @{name='wrong backend';change={param($o)$o.records[0].backend='another-driver'}},
 @{name='wrong OS';change={param($o)$o.operating_system='unreviewed Windows'}},
 @{name='wrong runtime';change={param($o)$o.winfsp_file_version='unreviewed runtime'}},
 @{name='extra row field';change={param($o)$o.records[0]|Add-Member -NotePropertyName extra -NotePropertyValue $null}},
 @{name='extra observation';change={param($o)$o.records[0].observations|Add-Member -NotePropertyName extra -NotePropertyValue $null}},
 @{name='null changed';change={param($o)$o.records[0].observations.rename_error=0}},
 @{name='missing null';change={param($o)$o.records[0].observations.PSObject.Properties.Remove('rename_error')}},
 @{name='boolean type';change={param($o)$o.records[0].observations.renamed='true'}},
 @{name='integer type';change={param($o)($o.records|Where-Object {$_.backend -ceq 'winfsp-legacy' -and $_.case -ceq 'ordinary-target'}).observations.rename_error='5'}}
)
foreach($case in $cases) {
 $actual=$raw|ConvertFrom-Json
 & $case.change $actual
 $rejected=$false
 try{Assert-WindowsReplacementReference -Fixture $fixture -Observation $actual}catch{$rejected=$true}
 if(-not $rejected){throw "Replacement checker accepted malformed data: $($case.name)"}
}
Write-Output 'PASS: twenty-one replacement records and twelve malformed-observation rejections'
