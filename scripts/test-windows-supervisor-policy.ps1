# Portable contract for the actual supervisor retry loop. This is NOT native
# Windows Task Scheduler, ACL, drive, or coordinator acceptance.
# Requires PowerShell 7 (Linux or Windows); child outcomes are real processes.
$ErrorActionPreference='Stop'
Set-StrictMode -Version Latest
$tokens=$null
$errors=$null
$ast=[System.Management.Automation.Language.Parser]::ParseFile((Join-Path $PSScriptRoot 'windows-session.ps1'),[ref]$tokens,[ref]$errors)
if($errors.Count){throw 'Supervisor source did not parse'}
foreach($name in @('Run-Mount','Log-Line')) {
    $function=$ast.FindAll({param($node) $node -is [System.Management.Automation.Language.FunctionDefinitionAst] -and $node.Name -eq $name},$true)
    if($function.Count -ne 1){throw 'Supervisor function missing or ambiguous'}
    . ([ScriptBlock]::Create($function[0].Extent.Text))
}
$root=Join-Path ([IO.Path]::GetTempPath()) ('s3drive-supervisor-'+[Guid]::NewGuid().ToString('N'))
$null=[IO.Directory]::CreateDirectory($root)
$stopFile=Join-Path $root 'stop'
$logFile=Join-Path $root 'policy.log'
$Component='Authority'
$engine=(Get-Process -Id $PID).Path
$script:attempts=0
$script:codes=@()
$script:interrupt=$false
$script:stopper=$null
$script:volumes=[Collections.Generic.List[string]]::new()
function Run-Child($Value) {
    $script:attempts++
    $script:volumes.Add($Value.authority.volume)
    # The same exclusive supervisor owner must remain held across every child.
    $duplicate=$null
    try {$duplicate=[IO.File]::Open((Join-Path $root 'supervisor.lock'),'Open','ReadWrite','None')} catch [IO.IOException] {}
    if($null -ne $duplicate){$duplicate.Dispose();throw 'Supervisor lost exclusive ownership'}
    $code=$script:codes[[Math]::Min($script:attempts-1,$script:codes.Count-1)]
    $info=[Diagnostics.ProcessStartInfo]::new()
    $info.FileName=$engine
    $info.Arguments="-NoProfile -NonInteractive -Command exit $code"
    $info.UseShellExecute=$false
    $child=[Diagnostics.Process]::Start($info)
    try {$child.WaitForExit();$actual=$child.ExitCode} finally {$child.Dispose()}
    if($script:interrupt) {
        $literal="'"+$stopFile.Replace("'","''")+"'"
        $command="Start-Sleep -Milliseconds 250; [IO.File]::WriteAllText($literal,'')"
        $info.Arguments='-NoProfile -NonInteractive -EncodedCommand '+[Convert]::ToBase64String([Text.Encoding]::Unicode.GetBytes($command))
        $script:stopper=[Diagnostics.Process]::Start($info)
    }
    return $actual
}
function Case([string]$Label,[int[]]$Codes,[int]$ExpectedCalls,[int]$ExpectedCode,[bool]$Interrupt=$false) {
    $script:attempts=0;$script:codes=$Codes;$script:interrupt=$Interrupt;$script:volumes.Clear()
    $value=[pscustomobject]@{authority=[pscustomobject]@{volume=('1'*32)}}
    $timer=[Diagnostics.Stopwatch]::StartNew()
    $result=Run-Mount $value
    if($result -ne $ExpectedCode -or $script:attempts -ne $ExpectedCalls){throw "Unexpected retry/exit contract: $Label"}
    if(@($script:volumes | Where-Object {$_ -ne $value.authority.volume}).Count){throw 'Retry changed pinned identity'}
    if([IO.File]::Exists($stopFile)){throw 'Supervisor left a stop marker after exit'}
    $released=[IO.File]::Open((Join-Path $root 'supervisor.lock'),'Open','ReadWrite','None')
    $released.Dispose()
    if($Interrupt -and $timer.Elapsed.TotalSeconds -ge 8){throw 'Stop did not interrupt the retry delay'}
    if($null -ne $script:stopper){$script:stopper.WaitForExit();$script:stopper.Dispose();$script:stopper=$null}
    Write-Output "PASS supervisor policy: $Label"
}
try {
    Case 'normal child exit is terminal' @(0) 1 0
    Case 'nonzero child exit recovers under the same owner' @(17,0) 2 0
    Case 'nonzero child exits exhaust four attempts' @(17) 4 17
    Case 'Stop interrupts retry without a new child' @(17) 1 0 $true
} finally {
    if($null -ne $script:stopper){$script:stopper.WaitForExit();$script:stopper.Dispose()}
    Remove-Item -LiteralPath $root -Recurse -Force
}
