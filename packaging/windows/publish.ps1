[CmdletBinding()]
param([Parameter(Mandatory)][string]$Tag, [string]$Directory = 'target/release/bundle')

$ErrorActionPreference = 'Stop'
$version = (Get-Content better-sonar/Cargo.toml | Select-String '^version = "(\d+\.\d+\.\d+)"$').Matches[0].Groups[1].Value
if ($Tag -cne "v$version") { throw "Tag '$Tag' must match better-sonar/Cargo.toml version '$version'" }
$repo = $env:GITHUB_REPOSITORY
if (-not $repo) { throw 'GITHUB_REPOSITORY is required' }
$nsis = Get-Item -LiteralPath (Join-Path $Directory "Better.Sonar_${version}_x64-setup.exe")
$msi = Get-Item -LiteralPath (Join-Path $Directory "Better.Sonar_${version}_x64_ko-KR.msi")
$notesJson = gh api --method POST "repos/$repo/releases/generate-notes" -f "tag_name=$Tag"
if ($LASTEXITCODE -ne 0) { throw 'Could not generate release notes' }
$notes = ($notesJson | ConvertFrom-Json).body

cargo run --locked -p better-sonar-sign -- packaging/windows/updater.pub $version $nsis.FullName $msi.FullName
if ($LASTEXITCODE -ne 0) { throw 'Could not sign the installers' }
$prefix = "https://github.com/$repo/releases/download/$Tag"
$nsisAsset = @{ url = "$prefix/$($nsis.Name)"; signature = (Get-Content "$($nsis.FullName).sig" -Raw).Trim() }
$msiAsset = @{ url = "$prefix/$($msi.Name)"; signature = (Get-Content "$($msi.FullName).sig" -Raw).Trim() }
$manifest = Join-Path $Directory latest.json
@{
    version = $version
    notes = $notes
    pub_date = (Get-Date).ToUniversalTime().ToString('yyyy-MM-ddTHH:mm:ssZ')
    platforms = @{
        'windows-x86_64' = $nsisAsset
        'windows-x86_64-nsis' = $nsisAsset
        'windows-x86_64-msi' = $msiAsset
    }
} | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath $manifest -Encoding utf8
$checksums = Join-Path $Directory SHA256SUMS.txt
@($nsis, $msi) | ForEach-Object {
    $hash = (Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
    "$hash  $($_.Name)"
} | Set-Content -LiteralPath $checksums -Encoding ascii

$assets = @($nsis.FullName, $msi.FullName, "$($nsis.FullName).sig", "$($msi.FullName).sig", $manifest, $checksums)
$notesFile = New-TemporaryFile
try {
    Set-Content -LiteralPath $notesFile.FullName -Value $notes -Encoding utf8
    gh release create $Tag --verify-tag --draft --title "Better Sonar $Tag" --notes-file $notesFile.FullName @assets
    if ($LASTEXITCODE -ne 0) { throw 'Could not create the draft release' }
} finally {
    Remove-Item -LiteralPath $notesFile.FullName
}
