# VibeStarter — Installateur

## Prérequis
- Windows 10 64-bit minimum
- [Rust + Cargo](https://rustup.rs)
- [Node.js 22+](https://nodejs.org)
- [NSIS](https://nsis.sourceforge.io) (installé automatiquement si absent)

## Build en 1 clic
Double-clique sur **BUILD.bat**

Ou depuis PowerShell :
```powershell
.\build-installer.ps1
```

## Ce que fait l'installateur
1. Vérifie Windows 64-bit et WebView2
2. Installe dans `%LOCALAPPDATA%\Vibe Starter\`
3. Crée un raccourci Bureau + Menu Démarrer
4. S'enregistre dans "Ajout/Suppression de programmes"

## Désinstallation
Via "Ajout/Suppression de programmes" ou `Uninstall.exe` dans le dossier d'installation.

## Structure dist/ attendue
```
installer/
  dist/
    vibe-starter.exe          ← binaire Tauri (cargo tauri build)
    resources/
      vm/
        vibestarter-guest.squashfs
        vmlinuz
        initrd.img
        qemu-system-x86_64.exe
        gvproxy.exe
        qemu-img.exe
        [DLLs QEMU...]
      RojoManagedPlugin.rbxm
```
