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
