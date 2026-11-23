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
$repoRoot = Split-Path -Parent $PSScriptRoot
$version = (Get-Content -LiteralPath (Join-Path $repoRoot 'Cargo.toml') | Select-String '^version = "(\d+\.\d+\.\d+)"$').Matches[0].Groups[1].Value
$Directory = (Resolve-Path -LiteralPath $Directory).Path
$legacy = 'https://github.com/wooseongDev/better-sonar/releases/download/v0.0.6'
$config = Join-Path ([Environment]::GetFolderPath('ApplicationData')) 'dev.wooseong.better-sonar/settings.json'
$nsisShortcut = Join-Path ([Environment]::GetFolderPath('Programs')) 'Better Sonar.lnk'
$msiShortcut = Join-Path ([Environment]::GetFolderPath('CommonPrograms')) 'Better Sonar.lnk'

function Get-MsiEntry {
    Get-ItemProperty "$machineUninstall\*" -ErrorAction SilentlyContinue |
        Where-Object { $_.DisplayName -eq 'Better Sonar' -and $_.WindowsInstaller -eq 1 }
}

function Invoke-Installer([string]$Program, [string]$Arguments) {
    $process = Start-Process -FilePath $Program -ArgumentList $Arguments -WindowStyle Hidden -Wait -PassThru
    if ($process.ExitCode -ne 0) { throw "$Program exited with $($process.ExitCode)" }
}

function Assert-Install([string]$Path, [string]$Kind, [string]$Shortcut) {
    $app = Get-Item -LiteralPath (Join-Path $Path 'better-sonar.exe')
    if ($app.VersionInfo.ProductVersion -ne $version) { throw "Unexpected app version $($app.VersionInfo.ProductVersion)" }
    $marker = (Get-Content -LiteralPath (Join-Path $Path 'installer-kind') -Raw).Trim()
    if ($marker -ne $Kind) { throw "Expected installer kind $Kind, found $marker" }
    if (-not (Test-Path -LiteralPath $Shortcut -PathType Leaf)) { throw "Missing Start menu shortcut: $Shortcut" }
}

function Assert-AppStarts([string]$Path) {
    $app = Start-Process -FilePath (Join-Path $Path 'better-sonar.exe') -WorkingDirectory $testDir.FullName -PassThru
    try {
        $deadline = [DateTime]::UtcNow.AddSeconds(30)
        do {
            $app.Refresh()
            if ($app.HasExited) { throw "Better Sonar exited with $($app.ExitCode) before showing its window" }
            if ($app.MainWindowTitle -eq 'Better Sonar') { return }
            Start-Sleep -Milliseconds 200
        } while ([DateTime]::UtcNow -lt $deadline)
        throw 'Better Sonar did not open its window'
    } finally {
        if (-not $app.HasExited) {
            $app.Kill()
            $app.WaitForExit()
        }
        $app.Dispose()
    }
}

function Assert-Removed([string]$Path, [string]$Shortcut) {
    if ((Test-Path -LiteralPath (Join-Path $Path 'better-sonar.exe')) -or (Test-Path -LiteralPath $Shortcut)) {
        throw "Uninstall left the app or its shortcut at $Path"
    }
}

if ((Test-Path -LiteralPath $uninstallKey) -or (Get-MsiEntry) -or (Test-Path -LiteralPath $config)) {
    throw 'Better Sonar already exists on this machine. Start with a clean VM.'
}
$tempRoot = if ($env:RUNNER_TEMP) { $env:RUNNER_TEMP } else { $env:TEMP }
$testDir = New-Item -ItemType Directory -Path (Join-Path $tempRoot 'better-sonar-upgrade') -Force
$newNsis = (Get-Item -LiteralPath (Join-Path $Directory "Better.Sonar_${version}_x64-setup.exe")).FullName
$newMsi = (Get-Item -LiteralPath (Join-Path $Directory "Better.Sonar_${version}_x64_ko-KR.msi")).FullName
$oldNsis = Join-Path $testDir.FullName 'old-setup.exe'
$oldMsi = Join-Path $testDir.FullName 'old-setup.msi'
Invoke-WebRequest "$legacy/Better.Sonar_0.0.6_x64-setup.exe" -OutFile $oldNsis
Invoke-WebRequest "$legacy/Better.Sonar_0.0.6_x64_ko-KR.msi" -OutFile $oldMsi
$msiexec = Join-Path $env:SystemRoot 'System32/msiexec.exe'

Invoke-Installer $newNsis '/S'
$installed = Get-ItemProperty -LiteralPath $uninstallKey
if ($installed.DisplayVersion -ne $version) { throw "Expected NSIS to install $version" }
Assert-Install $installed.InstallLocation 'nsis' $nsisShortcut
Assert-AppStarts $installed.InstallLocation
Invoke-Installer (Join-Path $installed.InstallLocation 'uninstall.exe') '/S'
if (Test-Path -LiteralPath $uninstallKey) { throw 'NSIS uninstall registration remains' }
Assert-Removed $installed.InstallLocation $nsisShortcut

Invoke-Installer $msiexec ("/i `"$newMsi`" /qn /norestart")
$installed = @(Get-MsiEntry)
if ($installed.Count -ne 1 -or $installed[0].DisplayVersion -ne $version) { throw "Expected MSI to install $version" }
Assert-Install $installed[0].InstallLocation 'msi' $msiShortcut
Assert-AppStarts $installed[0].InstallLocation
Invoke-Installer $msiexec ("/x $($installed[0].PSChildName) /qn /norestart")
if (Get-MsiEntry) { throw 'MSI uninstall registration remains' }
Assert-Removed $installed[0].InstallLocation $msiShortcut

$null = New-Item -ItemType Directory -Path (Split-Path -Parent $config) -Force
'{"headsetDeviceId":"headset","speakerDeviceId":"speaker","shortcut":"Ctrl+Alt+F9","autostart":false}' |
    Set-Content -LiteralPath $config
$settingsHash = (Get-FileHash -LiteralPath $config).Hash

Invoke-Installer $oldNsis '/S'
$before = Get-ItemProperty -LiteralPath $uninstallKey
if ($before.DisplayVersion -ne '0.0.6') { throw 'Expected NSIS to install 0.0.6' }
Invoke-Installer $newNsis '/P'
$after = Get-ItemProperty -LiteralPath $uninstallKey
if ($after.DisplayVersion -ne $version -or $after.InstallLocation -ne $before.InstallLocation.Trim('"')) {
    throw 'NSIS did not upgrade the existing install'
}
Assert-Install $after.InstallLocation 'nsis' $nsisShortcut
if ((Get-FileHash -LiteralPath $config).Hash -ne $settingsHash) { throw 'NSIS upgrade changed existing settings' }
Invoke-Installer (Join-Path $after.InstallLocation 'uninstall.exe') '/S'
if (Test-Path -LiteralPath $uninstallKey) { throw 'NSIS uninstall registration remains' }
Assert-Removed $after.InstallLocation $nsisShortcut
if ((Get-FileHash -LiteralPath $config).Hash -ne $settingsHash) { throw 'NSIS uninstall changed existing settings' }

Invoke-Installer $msiexec ("/i `"$oldMsi`" /qn /norestart")
$before = @(Get-MsiEntry)
if ($before.Count -ne 1 -or $before[0].DisplayVersion -ne '0.0.6') { throw 'Expected MSI to install 0.0.6' }
Invoke-Installer $msiexec ("/i `"$newMsi`" /qn /norestart")
$after = @(Get-MsiEntry)
if ($after.Count -ne 1 -or $after[0].DisplayVersion -ne $version -or $after[0].PSChildName -eq $before[0].PSChildName -or
    $after[0].InstallLocation.TrimEnd('\') -ne $before[0].InstallLocation.TrimEnd('\')) {
    throw 'MSI did not replace the previous product'
}
Assert-Install $after[0].InstallLocation 'msi' $msiShortcut
if ((Get-FileHash -LiteralPath $config).Hash -ne $settingsHash) { throw 'MSI upgrade changed existing settings' }
Invoke-Installer $msiexec ("/x $($after[0].PSChildName) /qn /norestart")
if (Get-MsiEntry) { throw 'MSI uninstall registration remains' }
Assert-Removed $after[0].InstallLocation $msiShortcut
if ((Get-FileHash -LiteralPath $config).Hash -ne $settingsHash) { throw 'MSI uninstall changed existing settings' }
