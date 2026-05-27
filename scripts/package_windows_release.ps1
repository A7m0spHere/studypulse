param(
  [string]$Version = "0.2.3"
)

$ErrorActionPreference = "Stop"

$root = Resolve-Path (Join-Path $PSScriptRoot "..")
$releaseRoot = Join-Path $root "release"
$packageDir = Join-Path $releaseRoot "StudyPulse_$($Version)_x64_cn"
$zipPath = Join-Path $releaseRoot "StudyPulse_$($Version)_x64_cn.zip"
$sourceMsi = Join-Path $root "src-tauri\target\release\bundle\msi\StudyPulse_$($Version)_x64_en-US.msi"
$friendlyMsi = Join-Path $packageDir "StudyPulse_Setup_$($Version)_x64.msi"
$certificate = Join-Path $root "StudyPulse-Test-Code-Signing.cer"
$manualTemplate = Join-Path $root "docs\studypulse_user_manual_cn.txt"
$sourceChangelog = Join-Path $root "CHANGELOG.md"
$manual = Join-Path $packageDir "StudyPulse_User_Manual_CN.txt"
$changelog = Join-Path $packageDir "StudyPulse_Changelog_CN.txt"

if (!(Test-Path -LiteralPath $sourceMsi)) {
  throw "MSI not found: $sourceMsi. Run npm run tauri build -- --bundles msi first."
}

if (!(Test-Path -LiteralPath $certificate)) {
  throw "Certificate not found: $certificate"
}

if (!(Test-Path -LiteralPath $manualTemplate)) {
  throw "Manual template not found: $manualTemplate"
}

if (!(Test-Path -LiteralPath $sourceChangelog)) {
  throw "Changelog not found: $sourceChangelog"
}

if (Test-Path -LiteralPath $packageDir) {
  Remove-Item -LiteralPath $packageDir -Recurse -Force
}
New-Item -ItemType Directory -Force -Path $packageDir | Out-Null

Copy-Item -LiteralPath $sourceMsi -Destination $friendlyMsi -Force
Copy-Item -LiteralPath $certificate -Destination (Join-Path $packageDir "StudyPulse-Test-Code-Signing.cer") -Force
Copy-Item -LiteralPath $sourceChangelog -Destination $changelog -Force

$utf8NoBom = New-Object System.Text.UTF8Encoding $false
$manualText = [System.IO.File]::ReadAllText($manualTemplate, [System.Text.Encoding]::UTF8).Replace("__VERSION__", $Version)
[System.IO.File]::WriteAllText($manual, $manualText, $utf8NoBom)

if (Test-Path -LiteralPath $zipPath) {
  Remove-Item -LiteralPath $zipPath -Force
}

Compress-Archive -Path (Join-Path $packageDir "*") -DestinationPath $zipPath -Force
Write-Host "Created: $zipPath"
