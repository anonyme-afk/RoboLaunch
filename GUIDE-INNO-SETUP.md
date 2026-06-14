# Guide — Créer ton installeur RoboLaunch avec Inno Setup

## 1. Télécharger Inno Setup

Va sur : https://jrsoftware.org/isdl.php
Télécharge **Inno Setup 6.x** (gratuit, environ 4 MB)
Installe-le normalement sur ton Windows.

---

## 2. Structure des fichiers nécessaires

Avant de compiler, vérifie que tu as cette structure :

```
installer/
├── robolaunch-setup.iss        ← le script (fourni)
├── assets/
│   ├── icon.ico                ← icône 256x256 (obligatoire)
│   ├── icon_small.bmp          ← 55x55 pixels (coin en-tête wizard)
│   └── sidebar.bmp             ← 164x314 pixels (image gauche wizard)
└── output/                     ← dossier créé automatiquement
    └── RoboLaunch-Setup-1.0.0.exe  ← résultat final
```

Et que le dossier **src-tauri/target/release/** contient :
```
robo-launch.exe
*.dll
WebView2Loader.dll
```

Et que **src-tauri/resources/vm/** contient les fichiers QEMU.

---

## 3. Compiler l'installeur

### Méthode A — Double-clic (le plus simple)
1. Clique droit sur `robolaunch-setup.iss`
2. Choisir **"Compile with Inno Setup"**
3. Attendre… le `.exe` apparaît dans `installer/output/`

### Méthode B — Ouvrir dans l'IDE Inno Setup
1. Lance **Inno Setup Compiler** depuis le menu Démarrer
2. **File → Open** → choisis `robolaunch-setup.iss`
3. Appuie sur **F9** ou le bouton ▶ pour compiler
4. Tu vois les logs en temps réel dans le bas de l'écran

### Méthode C — Ligne de commande (pour CI/CD)
```bat
"C:\Program Files (x86)\Inno Setup 6\ISCC.exe" installer\robolaunch-setup.iss
```

---

## 4. Ce que fait l'installeur (fonctionnalités incluses)

| Fonctionnalité | Détails |
|---|---|
| 📁 Dossier d'installation | `C:\Program Files\RoboLaunch\` |
| 🖥️ Raccourci Bureau | Coché par défaut (décoché possible) |
| 📋 Menu Démarrer | Groupe RoboLaunch + lien désinstalleur |
| 🔧 Registre Windows | Ajout PATH + infos Ajout/Suppression programmes |
| 🚀 Lancer après install | Proposé à la fin (checkbox) |
| ⚡ Démarrage auto Windows | Option non-cochée par défaut |
| 🗑️ Désinstalleur | Généré automatiquement + confirmation |
| 🌍 Langues | Français + Anglais (auto selon Windows) |
| 📦 Compression | LZMA2 ultra64 (max compression) |
| 🔒 Droits requis | Admin (pour installer dans Program Files) |
| ✅ Windows minimum | Windows 10 20H1 (build 19041) |

---

## 5. Créer les images du wizard (sidebar + icon)

### sidebar.bmp (image gauche de l'assistant)
- Taille exacte : **164 × 314 pixels**
- Format : BMP 24-bit
- Fond sombre recommandé (#0d0d0f) avec le logo RoboLaunch

**Outil gratuit :** Paint.NET ou GIMP
En GIMP : Image → Canvas Size → 164x314 → Export as BMP

### icon.ico (icône principale)
- Doit contenir plusieurs tailles : 16, 32, 48, 64, 128, 256
- Outil : IcoFX (gratuit) ou convertisseur en ligne favicon.io

### icon_small.bmp (coin supérieur droit du wizard)
- Taille exacte : **55 × 55 pixels**
- Format : BMP 24-bit

---

## 6. Tester l'installeur

1. Lance `RoboLaunch-Setup-1.0.0.exe`
2. Passe toutes les étapes
3. Vérifie que :
   - L'app se lance depuis le Bureau
   - Elle apparaît dans **Paramètres → Applications**
   - La désinstallation via le panneau de contrôle fonctionne

---

## 7. Erreurs fréquentes

| Erreur | Cause | Fix |
|---|---|---|
| `Source file not found` | Tauri pas encore build | Faire `cargo tauri build` avant |
| `Compiler version too old` | Inno Setup < 6 | Télécharger la v6 |
| `Invalid image size` | sidebar.bmp pas 164x314 | Redimensionner exactement |
| `Icon file not found` | icon.ico manquant | Créer/copier dans assets/ |
