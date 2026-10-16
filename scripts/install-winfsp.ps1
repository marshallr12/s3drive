$ErrorActionPreference = 'Stop'
# Isolated Windows CI runners only. User machines install WinFsp interactively.
$installer = Join-Path $env:RUNNER_TEMP 'winfsp-2.1.25156.msi'
Invoke-WebRequest 'https://github.com/winfsp/winfsp/releases/download/v2.1/winfsp-2.1.25156.msi' -OutFile $installer
$expected = '073a70e00f77423e34bed98b86e600def93393ba5822204fac57a29324db9f7a'
if ((Get-FileHash $installer -Algorithm SHA256).Hash.ToLowerInvariant() -ne $expected) {
    throw 'WinFsp installer digest mismatch'
}
$install = Start-Process msiexec.exe -ArgumentList @('/i', "`"$installer`"", '/qn', '/norestart', 'ADDLOCAL=ALL') -Wait -PassThru
if ($install.ExitCode -ne 0) { throw "WinFsp install failed or requires reboot: $($install.ExitCode)" }
$directory = (Get-ItemProperty 'HKLM:\SOFTWARE\WOW6432Node\WinFsp').InstallDir
(Get-Item (Join-Path $directory 'bin\winfsp-x64.dll')).VersionInfo | Format-List FileVersion, ProductVersion
# The official tests import winfsp-x64.dll directly (our app delay-loads it).
# Make the installed runtime available to this process and subsequent CI steps.
$runtimeDirectory = Join-Path $directory 'bin'
$env:PATH = "$runtimeDirectory;$env:PATH"
$runtimeDirectory | Out-File -FilePath $env:GITHUB_PATH -Append -Encoding utf8

$testsArchive = Join-Path $env:RUNNER_TEMP 'winfsp-tests-2.1.25156.zip'
Invoke-WebRequest 'https://github.com/winfsp/winfsp/releases/download/v2.1/winfsp-tests-2.1.25156.zip' -OutFile $testsArchive
if ((Get-FileHash $testsArchive -Algorithm SHA256).Hash.ToLowerInvariant() -ne '0cfc68791703c80f96c729ebd38b63187de53e19baa1c603a7059d6ea87c5f52') {
    throw 'WinFsp test archive digest mismatch'
}
$testsDirectory = Join-Path $env:RUNNER_TEMP 'winfsp-tests'
Expand-Archive $testsArchive -DestinationPath $testsDirectory -Force
$tests = @(Get-ChildItem $testsDirectory -Recurse -Filter 'winfsp-tests-x64.exe')
if ($tests.Count -ne 1) { throw 'Expected exactly one WinFsp x64 test executable' }
& $tests[0].FullName --external --list +getvolinfo_test
if ($LASTEXITCODE -ne 0) { throw "WinFsp test executable preflight failed: $LASTEXITCODE" }
"S3DRIVE_WINFSP_TEST_EXE=$($tests[0].FullName)" | Out-File -FilePath $env:GITHUB_ENV -Append -Encoding utf8
