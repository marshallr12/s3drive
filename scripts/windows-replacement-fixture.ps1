function Assert-WindowsReplacementReference {
    param([Parameter(Mandatory)]$Fixture,[Parameter(Mandatory)]$Observation)
    $cases=@('closed-target','ordinary-target','deferred-target','both-deferred','pending-target','cleared-deferred','duplicate-deferred')
    foreach($field in @('operating_system','winfsp_file_version')) {
        if($Observation.$field -cne $Fixture.$field) {throw "Replacement reference platform changed: $field"}
    }
    $actual=@($Observation.records);$expected=@($Fixture.records)
    if($actual.Count -ne 21 -or $expected.Count -ne 21) {throw 'Replacement reference requires twenty-one records'}
    foreach($backend in @('ntfs','winfsp-legacy','winfsp-posix')) {
        foreach($case in $cases) {
            $e=@($expected|Where-Object {$_.backend -ceq $backend -and $_.case -ceq $case})
            $a=@($actual|Where-Object {$_.backend -ceq $backend -and $_.case -ceq $case})
            if($a.Count -ne 1 -or $e.Count -ne 1) {throw "Missing/duplicate replacement case: $backend/$case"}
            $keys=@($e[0].observations.PSObject.Properties.Name);$observed=@($a[0].observations.PSObject.Properties.Name)
            if($keys.Count -ne $observed.Count) {throw "Replacement fields changed: $backend/$case"}
            foreach($key in $keys) {
                if($observed -cnotcontains $key) {throw "Missing replacement field: $key"}
                if((ConvertTo-Json -InputObject $a[0].observations.$key -Compress) -cne (ConvertTo-Json -InputObject $e[0].observations.$key -Compress)) {throw "Replacement observation changed: $backend/$case/$key"}
            }
        }
    }
}
