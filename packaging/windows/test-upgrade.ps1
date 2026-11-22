[CmdletBinding()]
param(
    [switch]$Isolated,
    [string]$Directory = 'target/release/bundle'
)

$ErrorActionPreference = 'Stop'
if (-not $Isolated -and $env:GITHUB_ACTIONS -ne 'true') {
    throw 'Run on a disposable Windows runner or VM and pass -Isolated.'
}

$uninstallKey = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\Better Sonar'
$machineUninstall = 'HKLM:\Software\Microsoft\Windows\CurrentVersion\Uninstall'
$version = (Get-Content Cargo.toml | Select-String '^version = "(\d+\.\d+\.\d+)"$').Matches[0].Groups[1].Value
$Directory = (Resolve-Path -LiteralPath $Directory).Path
$legacy = 'https://github.com/wooseongDev/better-sonar/releases/download/v0.0.6'
$config = Join-Path ([Environment]::GetFolderPath('ApplicationData')) 'dev.wooseong.better-sonar/settings.json'

function Get-MsiEntry {
    Get-ItemProperty "$machineUninstall\*" -ErrorAction SilentlyContinue |
        Where-Object { $_.DisplayName -eq 'Better Sonar' -and $_.WindowsInstaller -eq 1 }
}

function Invoke-Installer([string]$Program, [string]$Arguments) {
    $process = Start-Process -FilePath $Program -ArgumentList $Arguments -WindowStyle Hidden -Wait -PassThru
    if ($process.ExitCode -ne 0) { throw "$Program exited with $($process.ExitCode)" }
}

function Assert-Install([string]$Path, [string]$Kind) {
    $app = Get-Item -LiteralPath (Join-Path $Path 'better-sonar.exe')
    if ($app.VersionInfo.ProductVersion -ne $version) { throw "Unexpected app version $($app.VersionInfo.ProductVersion)" }
    $marker = (Get-Content -LiteralPath (Join-Path $Path 'installer-kind') -Raw).Trim()
    if ($marker -ne $Kind) { throw "Expected installer kind $Kind, found $marker" }
    if ((Get-FileHash -LiteralPath $config).Hash -ne $settingsHash) { throw 'Existing settings were changed' }
}

if ((Test-Path -LiteralPath $uninstallKey) -or (Get-MsiEntry) -or (Test-Path -LiteralPath $config)) {
    throw 'Better Sonar already exists on this machine. Start with a clean VM.'
}
$tempRoot = if ($env:RUNNER_TEMP) { $env:RUNNER_TEMP } else { $env:TEMP }
$testDir = New-Item -ItemType Directory -Path (Join-Path $tempRoot 'better-sonar-upgrade') -Force
$null = New-Item -ItemType Directory -Path (Split-Path -Parent $config) -Force
'{"headsetDeviceId":"headset","speakerDeviceId":"speaker","shortcut":"Ctrl+Alt+F9","autostart":false}' |
    Set-Content -LiteralPath $config
$settingsHash = (Get-FileHash -LiteralPath $config).Hash

$oldNsis = Join-Path $testDir.FullName 'old-setup.exe'
$oldMsi = Join-Path $testDir.FullName 'old-setup.msi'
Invoke-WebRequest "$legacy/Better.Sonar_0.0.6_x64-setup.exe" -OutFile $oldNsis
Invoke-WebRequest "$legacy/Better.Sonar_0.0.6_x64_ko-KR.msi" -OutFile $oldMsi
$newNsis = Join-Path $Directory "Better.Sonar_${version}_x64-setup.exe"
$newMsi = Join-Path $Directory "Better.Sonar_${version}_x64_ko-KR.msi"

Invoke-Installer $oldNsis '/S'
$before = Get-ItemProperty -LiteralPath $uninstallKey
if ($before.DisplayVersion -ne '0.0.6') { throw 'Expected NSIS to install 0.0.6' }
Invoke-Installer $newNsis '/P'
$after = Get-ItemProperty -LiteralPath $uninstallKey
if ($after.DisplayVersion -ne $version -or $after.InstallLocation -ne $before.InstallLocation.Trim('"')) {
    throw 'NSIS did not upgrade the existing install'
}
Assert-Install $after.InstallLocation 'nsis'
Invoke-Installer (Join-Path $after.InstallLocation 'uninstall.exe') '/S'
if (Test-Path -LiteralPath $uninstallKey) { throw 'NSIS uninstall registration remains' }

$msiexec = Join-Path $env:SystemRoot 'System32/msiexec.exe'
Invoke-Installer $msiexec ("/i `"$oldMsi`" /qn /norestart")
$before = @(Get-MsiEntry)
if ($before.Count -ne 1 -or $before[0].DisplayVersion -ne '0.0.6') { throw 'Expected MSI to install 0.0.6' }
Invoke-Installer $msiexec ("/i `"$newMsi`" /qn /norestart")
$after = @(Get-MsiEntry)
if ($after.Count -ne 1 -or $after[0].DisplayVersion -ne $version -or $after[0].PSChildName -eq $before[0].PSChildName -or
    $after[0].InstallLocation.TrimEnd('\') -ne $before[0].InstallLocation.TrimEnd('\')) {
    throw 'MSI did not replace the previous product'
}
Assert-Install $after[0].InstallLocation 'msi'
Invoke-Installer $msiexec ("/x $($after[0].PSChildName) /qn /norestart")
if (Get-MsiEntry) { throw 'MSI uninstall registration remains' }
