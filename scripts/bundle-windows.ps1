[CmdletBinding()]
param(
    [string]$OutputDirectory = 'target/release/bundle',
    [string]$Binary,
    [string]$NsisRoot = $env:BETTER_SONAR_NSIS_ROOT,
    [string]$WixRoot = $env:BETTER_SONAR_WIX_ROOT
)

$ErrorActionPreference = 'Stop'
$repo = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path

function Invoke-Checked([string]$Program, [string[]]$Arguments) {
    & $Program @Arguments
    if ($LASTEXITCODE -ne 0) { throw "$Program failed with exit code $LASTEXITCODE" }
}

function Find-Tool([string]$Name, [string[]]$Directories) {
    $command = Get-Command "$Name.exe" -ErrorAction SilentlyContinue
    if ($command) { return $command.Source }
    foreach ($directory in $Directories) {
        if ($directory) {
            $path = Join-Path $directory "$Name.exe"
            if (Test-Path -LiteralPath $path -PathType Leaf) { return $path }
        }
    }
    throw "$Name.exe was not found. Install NSIS and WiX 3 or specify their tool directories."
}

Push-Location $repo
try {
    $version = (Get-Content Cargo.toml | Select-String '^version = "(\d+\.\d+\.\d+)"$').Matches[0].Groups[1].Value
    if (-not $Binary) {
        Invoke-Checked cargo @('build', '--release', '--locked', '-p', 'better-sonar')
        $Binary = 'target/release/better-sonar.exe'
    }
    $Binary = (Resolve-Path -LiteralPath $Binary).Path
    if ((Get-Item -LiteralPath $Binary).VersionInfo.ProductVersion -ne $version) {
        throw "The executable must report product version $version."
    }
    $icon = (Resolve-Path assets/icon.ico).Path
    $output = (New-Item -Path $OutputDirectory -ItemType Directory -Force).FullName
    $nsis = Find-Tool makensis @($NsisRoot, "${env:ProgramFiles(x86)}\NSIS")
    $wix = @($WixRoot, "$env:WIX\bin", "${env:ProgramFiles(x86)}\WiX Toolset v3.14\bin")
    $candle = Find-Tool candle $wix
    $light = Find-Tool light $wix
    $setup = Join-Path $output "Better.Sonar_${version}_x64-setup.exe"
    $msi = Join-Path $output "Better.Sonar_${version}_x64_ko-KR.msi"

    Push-Location (Join-Path $repo 'packaging/windows')
    try {
        Invoke-Checked $nsis @("/DVERSION=$version", "/DOUTPUT=$setup", "/DBINARY=$Binary", "/DICON=$icon", 'installer.nsi')
        $object = Join-Path $output 'installer.wixobj'
        Invoke-Checked $candle @('-nologo', '-arch', 'x64', "-dVersion=$version", "-dBinary=$Binary", "-dIcon=$icon", '-out', $object, 'installer.wxs')
        Invoke-Checked $light @('-nologo', '-cultures:ko-KR', '-spdb', '-out', $msi, $object)
        Remove-Item -LiteralPath $object
    } finally {
        Pop-Location
    }
} finally {
    Pop-Location
}
