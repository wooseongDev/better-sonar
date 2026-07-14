[CmdletBinding()]
param(
  [Parameter(Mandatory = $true)]
  [string]$SourceRoot,

  [ValidateSet('Check', 'Run', 'Bundle')]
  [string]$Mode = 'Check',

  [Parameter(Mandatory = $true)]
  [string]$ResultPath,

  [string]$WorkRoot = '',

  [switch]$SkipInstall,

  [switch]$Clean
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
[Console]::OutputEncoding = [Text.UTF8Encoding]::new($false)
$OutputEncoding = [Text.UTF8Encoding]::new($false)

function Write-Step {
  param([Parameter(Mandatory = $true)][string]$Message)
  Write-Host "`n[windows-test] $Message" -ForegroundColor Cyan
}

function Get-RequiredCommandPath {
  param(
    [Parameter(Mandatory = $true)][string]$Name,
    [Parameter(Mandatory = $true)][string]$InstallHint
  )

  $command = Get-Command $Name -ErrorAction SilentlyContinue | Select-Object -First 1
  if ($null -eq $command) {
    throw "필수 명령 '$Name'을 찾을 수 없습니다. $InstallHint"
  }
  return $command.Source
}

function ConvertTo-NativeArgumentLine {
  param([string[]]$Arguments = @())

  $quoted = foreach ($argument in $Arguments) {
    if ($argument -notmatch '[\s"]') {
      $argument
    } else {
      ([char]34) + $argument.Replace('"', '\"') + ([char]34)
    }
  }
  return ($quoted -join ' ')
}

function Start-NativeProcess {
  param(
    [Parameter(Mandatory = $true)][string]$FilePath,
    [string[]]$Arguments = @()
  )

  $argumentLine = ConvertTo-NativeArgumentLine -Arguments $Arguments
  $workingDirectory = (Get-Location).ProviderPath
  $process = Start-Process -FilePath $FilePath -ArgumentList $argumentLine -WorkingDirectory $workingDirectory `
    -NoNewWindow -Wait -PassThru
  return $process.ExitCode
}

function Invoke-NativeCapture {
  param(
    [Parameter(Mandatory = $true)][string]$FilePath,
    [string[]]$Arguments = @()
  )

  $stdoutPath = [IO.Path]::GetTempFileName()
  $stderrPath = [IO.Path]::GetTempFileName()
  try {
    $argumentLine = ConvertTo-NativeArgumentLine -Arguments $Arguments
    $workingDirectory = (Get-Location).ProviderPath
    $process = Start-Process -FilePath $FilePath -ArgumentList $argumentLine -WorkingDirectory $workingDirectory `
      -NoNewWindow -Wait -PassThru `
      -RedirectStandardOutput $stdoutPath -RedirectStandardError $stderrPath
    return [pscustomobject]@{
      ExitCode = $process.ExitCode
      StdOut = [string](Get-Content -LiteralPath $stdoutPath -Raw -ErrorAction SilentlyContinue)
      StdErr = [string](Get-Content -LiteralPath $stderrPath -Raw -ErrorAction SilentlyContinue)
    }
  } finally {
    Remove-Item -LiteralPath $stdoutPath, $stderrPath -Force -ErrorAction SilentlyContinue
  }
}

function Import-MsvcEnvironment {
  $vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\vswhere.exe'
  if (-not (Test-Path -LiteralPath $vswhere -PathType Leaf)) {
    throw 'vswhere.exe를 찾을 수 없습니다. Visual Studio Build Tools의 Desktop development with C++ 워크로드를 설치하세요.'
  }

  $vswhereResult = Invoke-NativeCapture -FilePath $vswhere -Arguments @(
    '-latest',
    '-products',
    '*',
    '-requires',
    'Microsoft.VisualStudio.Component.VC.Tools.x86.x64',
    '-property',
    'installationPath'
  )
  $installationPath = @($vswhereResult.StdOut -split '\r?\n' | Where-Object { -not [string]::IsNullOrWhiteSpace($_) } | Select-Object -First 1)
  if ($vswhereResult.ExitCode -ne 0 -or $installationPath.Count -eq 0) {
    throw 'MSVC x64 빌드 도구를 찾을 수 없습니다. Desktop development with C++ 워크로드와 Windows SDK를 설치하세요.'
  }
  $installationPath = $installationPath[0].Trim()

  $vsDevCmd = Join-Path $installationPath 'Common7\Tools\VsDevCmd.bat'
  if (-not (Test-Path -LiteralPath $vsDevCmd -PathType Leaf)) {
    throw "Visual Studio 개발 환경 스크립트를 찾을 수 없습니다: $vsDevCmd"
  }

  $environmentScript = [IO.Path]::ChangeExtension([IO.Path]::GetTempFileName(), '.cmd')
  try {
    Set-Content -LiteralPath $environmentScript -Encoding ASCII -Value @(
      '@echo off',
      "call `"$vsDevCmd`" -no_logo -arch=x64 -host_arch=x64 >nul",
      'if errorlevel 1 exit /b %errorlevel%',
      'set'
    )
    $environmentResult = Invoke-NativeCapture -FilePath $env:ComSpec -Arguments @('/d', '/s', '/c', $environmentScript)
  } finally {
    Remove-Item -LiteralPath $environmentScript -Force -ErrorAction SilentlyContinue
  }
  if ($environmentResult.ExitCode -ne 0) {
    throw "MSVC 개발 환경을 불러오지 못했습니다: $vsDevCmd"
  }
  foreach ($line in ($environmentResult.StdOut -split '\r?\n')) {
    if ($line -match '^([^=]+)=(.*)$') {
      [Environment]::SetEnvironmentVariable($Matches[1], $Matches[2], 'Process')
    }
  }
  return $vsDevCmd
}

function Write-WindowsDriver {
  param(
    [Parameter(Mandatory = $true)][string]$Path,
    [Parameter(Mandatory = $true)][string]$ProjectRoot,
    [Parameter(Mandatory = $true)][string]$VsDevCmd,
    [Parameter(Mandatory = $true)][string]$Pnpm,
    [Parameter(Mandatory = $true)][string]$Cargo,
    [Parameter(Mandatory = $true)][string]$Rustc,
    [Parameter(Mandatory = $true)][string]$Git,
    [Parameter(Mandatory = $true)][string]$Node,
    [Parameter(Mandatory = $true)][string]$Mode,
    [switch]$SkipInstall
  )

  $lines = [Collections.Generic.List[string]]::new()
  $lines.Add('@chcp 65001 >nul')
  $lines.Add('@echo off')
  $lines.Add('setlocal EnableExtensions')
  $lines.Add("call `"$VsDevCmd`" -no_logo -arch=x64 -host_arch=x64")
  $lines.Add('if errorlevel 1 exit /b %errorlevel%')
  $lines.Add('set "SHELL=%COMSPEC%"')
  $lines.Add('set "npm_config_script_shell=%COMSPEC%"')
  $lines.Add('set "npm_config_manage_package_manager_versions=false"')
  $lines.Add("cd /d `"$ProjectRoot`"")
  $lines.Add("if not exist .git `"$Git`" init --quiet")
  $lines.Add('if errorlevel 1 exit /b %errorlevel%')
  $lines.Add("`"$Rustc`" -vV | findstr /R /C:`"host: .*pc-windows-msvc`" >nul")
  $lines.Add('if errorlevel 1 (echo [windows-test] Rust MSVC host toolchain is required. & exit /b 1)')
  $lines.Add("`"$Node`" --version")
  $lines.Add('if errorlevel 1 exit /b %errorlevel%')
  $lines.Add("call `"$Pnpm`" --version")
  $lines.Add('if errorlevel 1 exit /b %errorlevel%')

  if (-not $SkipInstall) {
    $lines.Add('echo [windows-test] Installing frontend dependencies...')
    $lines.Add("call `"$Pnpm`" install --frozen-lockfile")
    $lines.Add('if errorlevel 1 exit /b %errorlevel%')
  }

  $lines.Add('echo [windows-test] Running quality checks...')
  $lines.Add("call `"$Pnpm`" check:quality")
  $lines.Add('if errorlevel 1 exit /b %errorlevel%')
  $lines.Add("call `"$Pnpm`" build")
  $lines.Add('if errorlevel 1 exit /b %errorlevel%')
  $lines.Add("`"$Cargo`" test --workspace --locked")
  $lines.Add('if errorlevel 1 exit /b %errorlevel%')

  switch ($Mode) {
    'Check' {
      $lines.Add('echo [windows-test] Building the Windows debug application...')
      $lines.Add("call `"$Pnpm`" tauri build --debug --no-bundle -- --locked")
    }
    'Run' {
      $lines.Add('echo [windows-test] Starting the Windows Tauri development application...')
      $lines.Add("call `"$Pnpm`" tauri dev --no-watch -- --locked")
    }
    'Bundle' {
      $lines.Add('echo [windows-test] Building NSIS and MSI release bundles...')
      $lines.Add("call `"$Pnpm`" tauri build --bundles nsis,msi -- --locked")
    }
  }
  $lines.Add('if errorlevel 1 exit /b %errorlevel%')
  $lines.Add('echo [windows-test] Windows native test completed.')
  $lines.Add('exit /b 0')

  [IO.File]::WriteAllLines($Path, $lines, [Text.UTF8Encoding]::new($false))
}

function Initialize-ProtectedWorkRoot {
  param(
    [Parameter(Mandatory = $true)][string]$Path,
    [switch]$Reset
  )

  $markerName = '.better-sonar-wsl-windows-test'
  $marker = Join-Path $Path $markerName
  if ($Reset -and (Test-Path -LiteralPath $Path)) {
    if (-not (Test-Path -LiteralPath $marker -PathType Leaf)) {
      throw "보호 마커가 없는 폴더는 삭제하지 않습니다: $Path"
    }
    Remove-Item -LiteralPath $Path -Recurse -Force
  }

  if (-not (Test-Path -LiteralPath $Path)) {
    New-Item -ItemType Directory -Path $Path -Force | Out-Null
  } elseif (-not (Test-Path -LiteralPath $marker -PathType Leaf)) {
    $existing = @(Get-ChildItem -LiteralPath $Path -Force)
    if ($existing.Count -gt 0) {
      throw "기존 파일이 있는 작업 폴더에는 미러링하지 않습니다: $Path"
    }
  }

  Set-Content -LiteralPath $marker -Value 'Managed by scripts/test-windows.ps1' -Encoding ASCII
}

function Sync-SourceTree {
  param(
    [Parameter(Mandatory = $true)][string]$Robocopy,
    [Parameter(Mandatory = $true)][string]$From,
    [Parameter(Mandatory = $true)][string]$To
  )

  New-Item -ItemType Directory -Path $To -Force | Out-Null
  $arguments = @(
    $From,
    $To,
    '/MIR',
    '/FFT',
    '/R:2',
    '/W:1',
    '/NP',
    '/NJH',
    '/NJS',
    '/NFL',
    '/NDL',
    '/XD',
    '.git',
    'node_modules',
    'target',
    'dist',
    'artifacts',
    '/XF',
    '*.log'
  )
  $code = Start-NativeProcess -FilePath $Robocopy -Arguments $arguments
  if ($code -ge 8) {
    throw "소스 동기화에 실패했습니다. robocopy 종료 코드: $code"
  }
  Write-Host "[windows-test] 소스 동기화 완료(robocopy 종료 코드 $code)"
}

function Invoke-Main {
  $resolvedSource = (Resolve-Path -LiteralPath $SourceRoot).ProviderPath
  if (-not (Test-Path -LiteralPath (Join-Path $resolvedSource 'package.json') -PathType Leaf) -or
      -not (Test-Path -LiteralPath (Join-Path $resolvedSource 'src-tauri\tauri.conf.json') -PathType Leaf)) {
    throw "Better Sonar 저장소를 확인할 수 없습니다: $resolvedSource"
  }

  $resolvedWorkRoot = if ([string]::IsNullOrWhiteSpace($WorkRoot)) {
    Join-Path $env:LOCALAPPDATA 'BetterSonar\wsl-windows-test'
  } else {
    [IO.Path]::GetFullPath($WorkRoot)
  }
  if ($resolvedSource.TrimEnd('\') -eq $resolvedWorkRoot.TrimEnd('\')) {
    throw '원본 저장소를 테스트 작업 폴더로 사용할 수 없습니다.'
  }

  Initialize-ProtectedWorkRoot -Path $resolvedWorkRoot -Reset:$Clean
  $projectRoot = Join-Path $resolvedWorkRoot 'source'
  $logRoot = Join-Path $resolvedWorkRoot 'logs'
  New-Item -ItemType Directory -Path $logRoot -Force | Out-Null
  $logPath = Join-Path $logRoot ("{0}-{1}.log" -f (Get-Date -Format 'yyyyMMdd-HHmmss'), $Mode.ToLowerInvariant())

  $transcriptStarted = $false
  try {
    Start-Transcript -LiteralPath $logPath -Force | Out-Null
    $transcriptStarted = $true
    Set-Location -LiteralPath $resolvedWorkRoot

    Write-Step 'Windows 네이티브 도구 확인'
    $robocopy = Get-RequiredCommandPath -Name 'robocopy.exe' -InstallHint 'Windows 기본 시스템 파일을 확인하세요.'
    $vsDevCmd = Import-MsvcEnvironment
    $null = Get-RequiredCommandPath -Name 'cl.exe' -InstallHint 'Visual Studio C++ Build Tools를 설치하세요.'
    $null = Get-RequiredCommandPath -Name 'link.exe' -InstallHint 'Visual Studio C++ Build Tools를 설치하세요.'
    $cargo = Get-RequiredCommandPath -Name 'cargo.exe' -InstallHint 'Windows용 rustup과 stable-msvc 도구체인을 설치하세요.'
    $rustc = Get-RequiredCommandPath -Name 'rustc.exe' -InstallHint 'Windows용 rustup과 stable-msvc 도구체인을 설치하세요.'
    $git = Get-RequiredCommandPath -Name 'git.exe' -InstallHint 'Windows용 Git을 설치하세요.'
    $node = Get-RequiredCommandPath -Name 'node.exe' -InstallHint 'Windows에 Node.js를 설치하세요.'
    $pnpm = Get-RequiredCommandPath -Name 'pnpm.cmd' -InstallHint 'Windows에서 corepack enable 또는 pnpm 설치를 실행하세요.'

    $rustcResult = Invoke-NativeCapture -FilePath $rustc -Arguments @('-vV')
    $rustcVerbose = $rustcResult.StdOut
    if ($rustcResult.ExitCode -ne 0 -or $rustcVerbose -notmatch 'host: .*pc-windows-msvc') {
      throw "Rust MSVC 호스트 도구체인이 필요합니다.`n$rustcVerbose"
    }
    $nodeResult = Invoke-NativeCapture -FilePath $node -Arguments @('--version')
    $nodeVersion = $nodeResult.StdOut.Trim()
    if ($nodeResult.ExitCode -ne 0 -or $nodeVersion -notmatch '^v(?<major>\d+)') {
      throw "Node.js 버전을 확인하지 못했습니다: $nodeVersion"
    }
    $requiredNodeText = (Get-Content -LiteralPath (Join-Path $resolvedSource '.nvmrc') -Raw).Trim()
    $requiredNode = [int]($requiredNodeText.TrimStart('v'))
    if ([int]$Matches.major -lt $requiredNode) {
      throw "Node.js $requiredNode 이상이 필요하지만 현재 버전은 $nodeVersion 입니다."
    }
    $cargoResult = Invoke-NativeCapture -FilePath $cargo -Arguments @('--version')
    $cargoVersion = ([string]$cargoResult.StdOut).Trim()
    if ($cargoResult.ExitCode -ne 0 -or [string]::IsNullOrWhiteSpace($cargoVersion)) {
      throw "Cargo 버전을 확인하지 못했습니다: $($cargoResult.StdErr)"
    }
    Write-Host "[windows-test] Rust: $cargoVersion"
    Write-Host "[windows-test] Node: $nodeVersion"
    Write-Host "[windows-test] pnpm: $pnpm"

    Write-Step "WSL 소스를 Windows 작업 폴더로 동기화: $projectRoot"
    Sync-SourceTree -Robocopy $robocopy -From $resolvedSource -To $projectRoot
    if ($SkipInstall -and -not (Test-Path -LiteralPath (Join-Path $projectRoot 'node_modules') -PathType Container)) {
      throw '--skip-install을 사용하려면 Windows 작업 폴더에 기존 node_modules가 있어야 합니다.'
    }
    $driverPath = Join-Path $resolvedWorkRoot ("run-{0}.cmd" -f $Mode.ToLowerInvariant())
    Write-WindowsDriver -Path $driverPath -ProjectRoot $projectRoot -VsDevCmd $vsDevCmd -Pnpm $pnpm `
      -Cargo $cargo -Rustc $rustc -Git $git -Node $node -Mode $Mode -SkipInstall:$SkipInstall
    [IO.File]::WriteAllText($ResultPath, $driverPath, [Text.UTF8Encoding]::new($false))

    Write-Step 'Windows 네이티브 테스트 드라이버 준비 완료'
    Write-Host "[windows-test] 작업 폴더: $projectRoot"
    Write-Host "[windows-test] 드라이버: $driverPath"
    Write-Host "[windows-test] 로그: $logPath"
  } finally {
    if ($transcriptStarted) {
      Stop-Transcript | Out-Null
    }
  }
}

try {
  Invoke-Main
} catch {
  $detail = if ([string]::IsNullOrWhiteSpace($_.ScriptStackTrace)) {
    $_.Exception.Message
  } else {
    $_.Exception.Message + "`n" + $_.ScriptStackTrace
  }
  Write-Error ("[windows-test] 실패: " + $detail)
  exit 1
}
