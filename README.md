# RoboLaunch

Reconstruction complète (reverse engineering) de **VibeStarter** — une application Windows qui lance plusieurs agents IA (Claude, Codex, Agy, Aider) dans une VM Linux isolée (QEMU) et les connecte à Roblox Studio via MCP. Le projet a été reconstruit de zéro en Rust/Tauri 2.

> **Avertissement** : c'est un projet personnel de reverse engineering, pas une distribution officielle. Le dépôt contient le code source (Rust/Tauri + scripts invité), mais **pas** les binaires lourds nécessaires à l'exécution (QEMU, gvproxy, image disque Linux). Voir [Prérequis](#prérequis-avant-de-builder) ci-dessous.

---

## Architecture

```
Roblox Studio (Windows)
    ↕ mcp.bat (MCP JSON-RPC)
robo-launch.exe  ←→  HTTP Gateway :PORT  (Tauri/Rust, axum)
    ↕ TCP 10.0.2.2 (gvproxy) + token Bearer
roblox-mcp-proxy.cjs  (dans la VM Linux)
    ↕ stdio JSON-RPC
Claude Code / Codex / Agy / Aider  (agents dans la VM)
    ↕ MCP stdio
mcp-stub.cjs  →  HTTP POST /mcp/rpc  →  Tauri gateway
```

La VM tourne sous QEMU (`10.0.2.15`), `gvproxy` fait le pont réseau hôte↔invité (l'hôte est vu depuis la VM comme `10.0.2.2`), et l'authentification SSH se fait avec une clé ed25519 générée à chaque démarrage et transmise via `/proc/cmdline`.

---

## Structure du dépôt

```
RoboLaunch/
├── src-tauri/
│   ├── Cargo.toml               ← dépendances Rust
│   ├── tauri.conf.json
│   ├── build.rs
│   ├── icons/                   ← icônes de l'app (32x32, 128x128, .ico)
│   ├── capabilities/default.json
│   └── src/
│       ├── main.rs              ← bootstrap + commandes Tauri
│       ├── agent/mod.rs         ← AgentType, AgentInfo, AgentStatus
│       ├── vm/
│       │   ├── lifecycle.rs     ← QEMU, SSH, cycle de vie des agents, toolbox
│       │   └── ssh.rs           ← client SSH async (russh)
│       └── mcp/
│           ├── gateway.rs       ← HTTP /mcp/* (health/rpc/upload/close/notify)
│           └── roblox_proxy.rs  ← bridge TCP vers mcp.bat
├── frontend/
│   └── index.html               ← UI (tabs par agent, terminal, statut VM)
├── guest-scripts/                ← scripts installés dans l'image Linux invitée
├── installer/                    ← script Inno Setup + assets pour le .exe d'installation
├── build-squashfs.sh             ← réinjecte les guest-scripts dans l'image squashfs
└── README.md
```

---

## Prérequis avant de builder

RoboLaunch est une application **Windows** (elle pilote `qemu-system-x86_64.exe`, lance `cmd.exe /c mcp.bat`, etc.), buildée avec une toolchain Rust/Tauri standard. Pour la compiler et l'exécuter il faut :

- **Windows 10/11** avec [Rust](https://www.rust-lang.org/tools/install) (`rustup`) et le [Tauri CLI](https://tauri.app) (`cargo install tauri-cli` ou `cargo add tauri-cli --dev`).
- **Node.js** (pour le build du frontend, minimal ici : pas de framework, juste du HTML/JS statique).
- Les **binaires de VM** suivants, à placer dans `src-tauri/resources/vm/` (volontairement absents du dépôt — voir `.gitignore` — car trop lourds et potentiellement soumis à licence selon leur provenance) :
  - `qemu-system-x86_64.exe`, `qemu-img.exe`, `gvproxy.exe` + leurs `.dll`
  - `robolaunch-guest.squashfs` (image disque Linux contenant les agents et les scripts de `guest-scripts/`)
  - `vmlinuz`, `initrd.img`

Si tu pars d'une installation existante de VibeStarter, ces fichiers existent déjà chez toi (sous les noms `vibestarter-*`) ; il suffit de les copier et, pour le squashfs, de le reconstruire avec `build-squashfs.sh` pour y injecter les scripts RoboLaunch. Si tu n'as pas ces fichiers, il faut construire ta propre image Linux minimale (Alpine/Debian + systemd + Node.js + les CLIs des agents) — ce dépôt ne fournit pas cette image de base.

---

## Build

```bash
# 1. Cloner
git clone https://github.com/anonyme-afk/RoboLaunch.git
cd RoboLaunch

# 2. Frontend (génère frontend/dist/, requis par tauri::generate_context!())
cd frontend && npm run build && cd ..

# 3. Placer les binaires VM (voir Prérequis ci-dessus)
#    → src-tauri/resources/vm/

# 4. Build Tauri
cd src-tauri
cargo check          # vérification rapide sans tout compiler
cargo tauri build     # build complet + bundle

# 5. Installeur Windows (optionnel)
#    Installer Inno Setup 6 (jrsoftware.org/isdl.php), puis :
installer\BUILD.bat
#    → installer/output/RoboLaunch-Setup-1.0.0.exe
```

Pour reconstruire l'image squashfs invitée après une modification des `guest-scripts/` :
```bash
# Sous Linux/WSL (nécessite squashfs-tools : apt install squashfs-tools)
bash build-squashfs.sh /chemin/vers/image-de-base.squashfs
# → robolaunch-guest-rebuilt.squashfs, à renommer en robolaunch-guest.squashfs
```

---

## Audit & corrections (cette passe)

Le code avait été reconstruit avec une architecture globalement correcte, mais plusieurs bugs empêchaient le fonctionnement réel. Voici ce qui a été corrigé :

| Bug | Symptôme | Correctif |
|---|---|---|
| `tracing = "1"` dans `Cargo.toml` | Cette version n'existe pas sur crates.io, `cargo` échoue à la résolution des dépendances | `tracing = "0.1"` |
| Le token MCP d'un agent n'était jamais enregistré côté gateway (`register_token` n'était appelée nulle part) | Toutes les requêtes authentifiées d'un agent échouaient en `401 Unauthorized` | `GatewayState` conservé dans `AppState`, `register_token` appelée juste après `launch_agent` dans `cmd_launch_agent` |
| `auth_agent` ne lisait que `X-RoboLaunch-Token` / `X-VibeStarter-Token`, alors que `mcp-stub.cjs` envoie `Authorization: Bearer <token>` | Authentification systématiquement refusée | Lecture du header `Authorization` en priorité |
| `/mcp/rpc` n'appelait jamais `auth_agent` | Endpoint ouvert à quiconque atteint le port local de la gateway | Vérification d'auth ajoutée, `401` sinon |
| `upload_file` attendait du `multipart/form-data` (extracteur `Multipart`) alors que le stub envoie des octets bruts (`application/octet-stream`) | L'upload de fichier échouait toujours | Extracteur remplacé par `Bytes`, nom de fichier lu depuis `X-File-Name` |
| Dans `roblox_proxy.rs`, la map `pending` (id JSON-RPC → canal de réponse) était créée mais jamais alimentée ; le `reply_tx` des requêtes venant de la gateway était silencieusement jeté | Toute requête `/mcp/rpc` forwardée à Roblox Studio attendait 30s puis timeout, sans jamais recevoir la réponse de `mcp.bat` | `pending` partagée (Arc) entre la tâche d'écriture (qui y insère avant d'écrire) et la tâche de lecture (qui y retire pour router la réponse) |
| `build-squashfs.sh` référençait encore les anciens scripts `vibestarter-*` alors que `guest-scripts/` contient des fichiers `robolaunch-*` | Le script échouait à installer les scripts dans l'image (fichiers introuvables) ou installait les mauvais noms | Toutes les références renommées en `robolaunch-*`, cohérent avec `guest-scripts/` et avec ce que `lifecycle.rs` attend (`robolaunch-guest.squashfs`) |
| `tauri.conf.json` référençait `icons/32x32.png`, `icons/128x128.png`, `icons/icon.ico` qui n'existaient pas dans le dépôt | `cargo tauri build` échoue (fichier d'icône introuvable) | Icônes générées et ajoutées dans `src-tauri/icons/` |
| `installer/robolaunch-setup.iss` référençait `assets\icon_small.bmp`, absent du dépôt | La compilation de l'installeur avec Inno Setup échoue | Image ajoutée dans `installer/assets/icon_small.bmp` |
| Nom d'agent interpolé sans échappement dans la commande shell SSH (`-E ROBOLAUNCH_AGENT_NAME='{name}'`) | Une apostrophe dans le nom casse la commande envoyée à la VM | Échappement basique des apostrophes avant interpolation |
| `GUEST_IP` défini mais jamais utilisé, adresse dupliquée en dur dans `launch_gvproxy` | Incohérence / risque de désynchronisation si l'adresse change | `launch_gvproxy` réutilise la constante `GUEST_IP` |
| `lifecycle.rs::launch_agent` dupliquait le mapping type→binaire déjà présent dans `agent::AgentType::binary()` | Duplication de logique, `AgentType::binary()`/`FromStr` jamais utilisées | `launch_agent` utilise désormais `agent_type.parse::<AgentType>()?.binary()` |

**Vérification** : le code a été compilé avec succès (`cargo check`, profil dev) dans un environnement Linux sandboxé avec rustc/cargo 1.75 — ce qui confirme l'absence d'erreur de type ou de signature d'API sur les correctifs ci-dessus. Cet environnement n'a en revanche pas pu valider un `cargo tauri build` complet ni l'exécution réelle sur Windows (QEMU, gvproxy, et la VM invitée n'ont pas pu être testés ici) : un test réel sur ta machine Windows reste nécessaire avant de considérer le projet pleinement fonctionnel.

Points laissés tels quels (vérifiés non bugués) :
- `--uid={GUEST_USER}` dans `systemd-run` : `--uid` accepte un nom d'utilisateur, pas seulement un UID numérique — ce n'était pas un bug.
- `$schema` dans `capabilities/default.json` pointe vers un fichier généré localement par `tauri dev`/`tauri build` (non commité) — comportement standard d'un projet Tauri, pas une erreur.

---

## Nouvelles fonctionnalités vs VibeStarter

- `/mcp/notify` — endpoint de notifications push agent → host
- Toolbox API (`/mcp/rpc` → `robolaunch/listToolbox`)
- Tab labels personnalisables par agent
- Frontend : tabs dynamiques, terminal par agent, statut VM live, UI sombre

---

## Clés API

Les clés ne sont **jamais** hardcodées dans le code. Elles se configurent au lancement de chaque agent, via variables d'environnement dans la VM :

- **Claude** : `ANTHROPIC_API_KEY`
- **Codex** : `OPENAI_API_KEY`
- **Agy** : `CODEIUM_API_KEY` (ou `agy login`)
- **Aider** : `OPENAI_API_KEY` (ou tout modèle compatible)

Passe-les via la config agent dans l'UI, ou injecte-les dans l'image squashfs invitée pour qu'elles soient persistantes.

---

## Licence

Projet personnel non affilié à Roblox Corporation ni à l'éditeur original de VibeStarter. Utilisation à tes risques.
