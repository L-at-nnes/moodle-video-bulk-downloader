#requires -version 5.1
<#
Builds mvbd-cli and the Tauri GUI in release mode, pre-fetches the bundled
Chromium, and assembles a self-contained MVBD-Portable/ folder - no Rust,
no Cargo, no installer needed on the target machine.
#>
param(
    [switch]$SkipZip
)

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root

$portable = Join-Path $root "MVBD-Portable"
$portableTools = Join-Path $portable "tools"

Write-Host "==> Building mvbd-cli (release)" -ForegroundColor Cyan
cargo build --release -p mvbd-cli
if ($LASTEXITCODE -ne 0) { throw "cargo build -p mvbd-cli failed" }

Write-Host "==> Building GUI (release)" -ForegroundColor Cyan
Push-Location "crates/gui/src-tauri"
try {
    cargo tauri build
    if ($LASTEXITCODE -ne 0) { throw "cargo tauri build failed" }
} finally {
    Pop-Location
}

Write-Host "==> Fetching ffmpeg / mkvmerge / chromium into tools/" -ForegroundColor Cyan
cargo run --release -p mvbd-cli -- --ensure-tools
if ($LASTEXITCODE -ne 0) { throw "tool provisioning failed" }

Write-Host "==> Assembling $portable" -ForegroundColor Cyan
if (Test-Path $portable) { Remove-Item $portable -Recurse -Force }
New-Item -ItemType Directory -Path $portable | Out-Null
New-Item -ItemType Directory -Path $portableTools | Out-Null

Copy-Item "target/release/mvbd.exe" (Join-Path $portable "mvbd-cli.exe")
Copy-Item "target/release/mvbd-gui.exe" (Join-Path $portable "mvbd-gui.exe")

Copy-Item "tools/ffmpeg.exe" $portableTools
Copy-Item "tools/mkvmerge.exe" $portableTools
Copy-Item "tools/chromium" $portableTools -Recurse

Copy-Item "icon.ico" $portable
Copy-Item "LICENSE" $portable
Copy-Item "README.md" $portable

@"
Moodle Video Bulk Downloader - version portable

- mvbd-gui.exe   : interface graphique
- mvbd-cli.exe   : ligne de commande (mvbd-cli.exe --help)
- tools/         : ffmpeg, mkvmerge et Chromium embarqués, ne rien supprimer

Colle tes cookies dans un fichier cookies.txt a cote de ces .exe avant de lancer
un telechargement. Voir README.md pour le detail du format.
"@ | Out-File -Encoding utf8 (Join-Path $portable "LISEZMOI.txt")

if (-not $SkipZip) {
    Write-Host "==> Zipping MVBD-Portable.zip" -ForegroundColor Cyan
    $zipPath = Join-Path $root "MVBD-Portable.zip"
    if (Test-Path $zipPath) { Remove-Item $zipPath }
    Compress-Archive -Path $portable -DestinationPath $zipPath
}

Write-Host "==> Done: $portable" -ForegroundColor Green
