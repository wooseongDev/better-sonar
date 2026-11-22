[CmdletBinding()]
param([string]$Binary = 'target/debug/better-sonar.exe')

$ErrorActionPreference = 'Stop'
$Binary = (Resolve-Path -LiteralPath $Binary).Path
if (Test-Path -LiteralPath (Join-Path (Split-Path -Parent $Binary) 'installer-kind')) {
    throw 'Use an unpackaged build to keep the installed app settings isolated.'
}

Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;

public static class SonarWindow {
    [DllImport("user32.dll", SetLastError = true)]
    public static extern bool PostMessage(IntPtr window, uint message, IntPtr wParam, IntPtr lParam);
}
'@

function Start-App([bool]$Autostart) {
    $start = [Diagnostics.ProcessStartInfo]::new($Binary)
    $start.UseShellExecute = $false
    $start.Environment['APPDATA'] = $fixture.FullName
    if ($Autostart) { $start.ArgumentList.Add('--autostart') }
    [Diagnostics.Process]::Start($start)
}

function Wait-ForWindow([Diagnostics.Process]$App, [bool]$Visible) {
    for ($i = 0; $i -lt 100; $i++) {
        $App.Refresh()
        if ($App.HasExited) { throw "GPUI application exited with $($App.ExitCode)" }
        if (($App.MainWindowHandle -ne [IntPtr]::Zero) -eq $Visible) {
            return $App.MainWindowHandle
        }
        Start-Sleep -Milliseconds 100
    }
    throw "The GPUI window did not reach the expected visibility ($Visible)"
}

$fixture = New-Item -ItemType Directory -Path (Join-Path $env:TEMP ('better-sonar-smoke-' + [guid]::NewGuid()))
$app = $null
$second = $null
try {
    $settings = New-Item -Path (Join-Path $fixture.FullName 'dev.wooseong.better-sonar.dev') -ItemType Directory
    '{"shortcut":"Ctrl+Alt+Shift+F12","autostart":false,"mediaKeysEnabled":false}' |
        Set-Content -LiteralPath (Join-Path $settings.FullName 'settings.json')
    $app = Start-App $true
    Start-Sleep -Milliseconds 700
    $null = Wait-ForWindow $app $false
    $second = Start-App $false
    if (-not $second.WaitForExit(10000) -or $second.ExitCode -ne 0) {
        throw 'The second instance did not exit successfully'
    }
    $window = Wait-ForWindow $app $true
    if ($app.MainWindowTitle -ne 'Better Sonar') { throw "Unexpected window title: $($app.MainWindowTitle)" }

    if (-not [SonarWindow]::PostMessage($window, 0x0010, [IntPtr]::Zero, [IntPtr]::Zero)) {
        throw 'Could not request window close'
    }
    $null = Wait-ForWindow $app $false
    $second = Start-App $false
    if (-not $second.WaitForExit(10000) -or $second.ExitCode -ne 0) {
        throw 'The instance launched after closing the window did not exit successfully'
    }
    $window = Wait-ForWindow $app $true
    'GPUI tray startup, single instance, close to tray, and window reopen passed.'
} finally {
    foreach ($process in @($second, $app)) {
        if ($null -ne $process -and -not $process.HasExited) { Stop-Process -Id $process.Id }
    }
    $file = Join-Path $fixture.FullName 'dev.wooseong.better-sonar.dev/settings.json'
    if (Test-Path -LiteralPath $file) { Remove-Item -LiteralPath $file }
    $subfolder = Split-Path -Parent $file
    if (Test-Path -LiteralPath $subfolder) { Remove-Item -LiteralPath $subfolder }
    Remove-Item -LiteralPath $fixture.FullName
}
