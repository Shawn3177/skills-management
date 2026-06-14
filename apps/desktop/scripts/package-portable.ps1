$ErrorActionPreference = "Stop"

$scriptRoot = Split-Path -Parent $MyInvocation.MyCommand.Path
$appRoot = Split-Path -Parent $scriptRoot
$srcTauriRoot = Join-Path $appRoot "src-tauri"
$releaseRoot = Join-Path $srcTauriRoot "target\release"
$exePath = Join-Path $releaseRoot "skills-manage-desktop.exe"

if (-not (Test-Path -LiteralPath $exePath)) {
  throw "Release executable not found. Run 'npm run tauri build' from apps/desktop first."
}

$packageJson = Get-Content -LiteralPath (Join-Path $appRoot "package.json") -Raw | ConvertFrom-Json
$version = $packageJson.version
$portableRoot = Join-Path $releaseRoot "portable"
$stagingDir = Join-Path $portableRoot "Skills Manage"
$bundleRoot = Join-Path $releaseRoot "bundle\portable"
$zipPath = Join-Path $bundleRoot "Skills Manage_$version`_x64-portable.zip"

function Assert-ChildPath {
  param(
    [Parameter(Mandatory = $true)][string]$Child,
    [Parameter(Mandatory = $true)][string]$Parent
  )

  $fullChild = [System.IO.Path]::GetFullPath($Child)
  $fullParent = [System.IO.Path]::GetFullPath($Parent)

  if (-not $fullChild.StartsWith($fullParent, [System.StringComparison]::OrdinalIgnoreCase)) {
    throw "Refusing to operate outside expected folder: $fullChild"
  }
}

New-Item -ItemType Directory -Force -Path $portableRoot | Out-Null
New-Item -ItemType Directory -Force -Path $bundleRoot | Out-Null

Assert-ChildPath -Child $stagingDir -Parent $portableRoot
if (Test-Path -LiteralPath $stagingDir) {
  Remove-Item -LiteralPath $stagingDir -Recurse -Force
}

New-Item -ItemType Directory -Force -Path $stagingDir | Out-Null
Copy-Item -LiteralPath $exePath -Destination (Join-Path $stagingDir "Skills Manage.exe")

$readme = @"
Skills Manage Portable

Run "Skills Manage.exe" directly after extracting this folder.

Local data is stored outside the portable folder at:
%USERPROFILE%\.skills-manage

Deleting the portable folder removes the app files only. It does not remove your local skills library or backups.
"@

Set-Content -LiteralPath (Join-Path $stagingDir "README.txt") -Value $readme -Encoding UTF8

Assert-ChildPath -Child $zipPath -Parent $bundleRoot
if (Test-Path -LiteralPath $zipPath) {
  Remove-Item -LiteralPath $zipPath -Force
}

Compress-Archive -LiteralPath $stagingDir -DestinationPath $zipPath -Force

Write-Output "Created portable ZIP: $zipPath"
