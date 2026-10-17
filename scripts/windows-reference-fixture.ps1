# Pure reference-data validation; does not open native handles or run commands.
function Assert-WindowsDispositionReference {
    param([Parameter(Mandatory)]$Fixture, [Parameter(Mandatory)]$Observation)
    $expectedRecords = @($Fixture.records)
    $actualRecords = @($Observation.records)
    if ($expectedRecords.Count -ne 14 -or @($expectedRecords.case | Sort-Object -Unique).Count -ne 14) { throw 'Invalid fourteen-case reference fixture' }
    if ($actualRecords.Count -ne 14 -or @($actualRecords.case | Sort-Object -Unique).Count -ne 14) { throw 'Missing or duplicate reference observations' }
    if ($Observation.operating_system -cne $Fixture.operating_system) { throw 'Reference OS profile changed; measure and review before publication' }
    foreach ($expected in $expectedRecords) {
        if ($expected.backend -cne 'ntfs') { throw 'Invalid reference backend' }
        $matchingRecords = @($actualRecords | Where-Object { $_.case -ceq $expected.case })
        if ($matchingRecords.Count -ne 1 -or $matchingRecords[0].backend -cne $expected.backend) { throw "Reference case missing or changed: $($expected.case)" }
        $actual = $matchingRecords[0].observations
        $keys = @($expected.observations.PSObject.Properties.Name)
        $actualKeys = @($actual.PSObject.Properties.Name)
        if ($actualKeys.Count -ne $keys.Count) { throw "Reference fields changed: $($expected.case)" }
        foreach ($key in $keys) {
            if ($actualKeys -cnotcontains $key) { throw "Reference field missing: $($expected.case).$key" }
            $actualValue = ConvertTo-Json -InputObject $actual.$key -Compress
            $expectedValue = ConvertTo-Json -InputObject $expected.observations.$key -Compress
            if ($actualValue -cne $expectedValue) { throw "Reference observation changed: $($expected.case).$key" }
        }
    }
}
