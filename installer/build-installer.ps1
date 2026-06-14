# build-installer.ps1 — Build l'installateur VibeStarter sur Windows
# Prérequis : NSIS installé (https://nsis.sourceforge.io)
# Usage : .\build-installer.ps1

$ErrorActionPreference = "Stop"

Write-Host "=== VibeStarter Installer Builder ===" -ForegroundColor Cyan

# 1. Vérifier NSIS
$nsis = Get-Command "makensis" -ErrorAction SilentlyContinue
if (-not $nsis) {
    $nsis = "C:\Program Files (x86)\NSIS\makensis.exe"
    if (-not (Test-Path $nsis)) {
        Write-Host "NSIS non trouvé. Installation via winget..." -ForegroundColor Yellow
        winget install NSIS.NSIS
        $nsis = "C:\Program Files (x86)\NSIS\makensis.exe"
    }
} else {
    $nsis = $nsis.Source
}
Write-Host "NSIS: $nsis" -ForegroundColor Green

# 2. Build Tauri
Write-Host "`nBuild de l'app Tauri..." -ForegroundColor Cyan
Push-Location "..\src-tauri"
cargo tauri build
if ($LASTEXITCODE -ne 0) { throw "Build Tauri échoué" }
Pop-Location

# 3. Copier les binaires dans dist/
Write-Host "`nPréparation du dossier dist/..." -ForegroundColor Cyan
$dist = "dist"
Remove-Item -Recurse -Force $dist -ErrorAction SilentlyContinue
New-Item -ItemType Directory $dist | Out-Null

# Binaire principal (Tauri output)
$tauriOut = "..\src-tauri\target\release\vibe-starter.exe"
Copy-Item $tauriOut "$dist\vibe-starter.exe"

# Ressources VM (dossier resources/ du projet)
$resourcesVM = "..\src-tauri\resources"
if (Test-Path $resourcesVM) {
    Copy-Item -Recurse $resourcesVM "$dist\resources"
} else {
    Write-Host "ATTENTION: dossier resources/ absent — copiez manuellement les fichiers VM" -ForegroundColor Yellow
    New-Item -ItemType Directory "$dist\resources\vm" | Out-Null
}

# 4. Assets de l'installateur
Write-Host "`nPréparation des assets installateur..." -ForegroundColor Cyan
$assets = "assets"
if (-not (Test-Path "$assets\icon.ico")) {
    Write-Host "Copie d'une icône placeholder..." -ForegroundColor Yellow
    # Crée un fichier ICO minimal si absent
    [byte[]]$ico = @(0,0,1,0,1,0,16,16,0,0,1,0,32,0,104,4,0,0,22,0,0,0)
    [System.IO.File]::WriteAllBytes("$assets\icon.ico", $ico)
}
if (-not (Test-Path "$assets\LICENSE.txt")) {
    Set-Content "$assets\LICENSE.txt" "Copyright 2026 VibeStarter. All rights reserved."
}
if (-not (Test-Path "$assets\sidebar.bmp")) {
    # BMP 164x314 noir (placeholder)
    Write-Host "sidebar.bmp absent — utilise un vrai BMP 164x314px pour un beau look" -ForegroundColor Yellow
    # On crée un placeholder minimal
    $bmpHeader = [byte[]](0x42,0x4D,0x36,0x70,0x01,0x00,0x00,0x00,0x00,0x00,0x36,0x00,0x00,0x00,0x28,0x00,0x00,0x00,0xA4,0x00,0x00,0x00,0x3A,0x01,0x00,0x00,0x01,0x00,0x18,0x00,0x00,0x00,0x00,0x00,0x00,0x70,0x01,0x00,0x00,0x00,0x00,0x00,0x00,0x00,0x00,0x00,0x00,0x00,0x00,0x00,0x00,0x00,0x00,0x00)
    [System.IO.File]::WriteAllBytes("$assets\sidebar.bmp", $bmpHeader)
}

# 5. Build NSIS
Write-Host "`nCompilation de l'installateur..." -ForegroundColor Cyan
& $nsis "installer.nsi"
if ($LASTEXITCODE -ne 0) { throw "NSIS échoué" }

$output = Get-Item "VibeStarter-Setup-*.exe" | Select-Object -Last 1
$sizeMB = [math]::Round($output.Length / 1MB, 1)
Write-Host "`n✓ Installateur créé : $($output.Name) ($sizeMB MB)" -ForegroundColor Green
Write-Host "  → Distribue ce fichier à tes utilisateurs." -ForegroundColor Cyan
