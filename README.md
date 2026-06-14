# RoboLaunch 

**Fork/rebuild de VibeStarter** — Lance plusieurs agents IA (Claude, Codex, Agy, Aider) dans une VM Linux isolée (QEMU) et les connecte à Roblox Studio via MCP.

---

## Architecture

```
Roblox Studio (Windows)
    ↕ mcp.bat (MCP JSON-RPC)
robo-launch.exe  ←→  HTTP Gateway :PORT  (Tauri/Rust)
    ↕ TCP 10.0.2.2 + auth token
roblox-mcp-proxy.cjs  (dans VM Linux)
    ↕ stdio JSON-RPC
Claude Code / Codex / Agy / Aider  (agents VM)
    ↕ MCP stdio
mcp-stub.cjs  →  HTTP POST /mcp/rpc  →  Tauri gateway
```

---

## Structure

```
robolaunch/
├── src-tauri/
│   ├── Cargo.toml               ← dépendances Rust corrigées (async-trait, russh…)
│   ├── tauri.conf.json
│   ├── build.rs
│   └── src/
│       ├── main.rs              ← bootstrap + commandes Tauri
│       ├── agent/mod.rs         ← AgentType, AgentInfo, AgentStatus
│       ├── vm/
│       │   ├── lifecycle.rs     ← QEMU, SSH, agents, toolbox
│       │   └── ssh.rs           ← client russh async
│       └── mcp/
│           ├── gateway.rs       ← HTTP /mcp/* (health/rpc/upload/close/notify)
│           └── roblox_proxy.rs  ← bridge TCP mcp.bat (race condition corrigée)
├── frontend/
│   └── index.html               ← UI agents (4 types, tabs, terminal, pause/kill)
├── guest-scripts/               ← scripts VM renommés robolaunch-*
├── installer/
│   ├── BUILD.bat
│   └── installer.nsi
└── README.md
```

---

## Build

```bash
# 1. Frontend
cd frontend && npm run build

# 2. Tauri
cd src-tauri && cargo tauri build

# 3. Installeur Windows (double-clic)
installer/BUILD.bat
```

---

## Avant de builder

Copie depuis ton install VibeStarter existant :
```
src-tauri/resources/vm/
  ├── qemu-system-x86_64.exe
  ├── qemu-img.exe
  ├── gvproxy.exe
  ├── *.dll (QEMU dépendances)
  ├── robolaunch-guest.squashfs   ← renommé depuis vibestarter-guest.squashfs
  ├── vmlinuz
  └── initrd.img
```

Pour rebuilder le squashfs avec les scripts mis à jour :
```bash
bash build-squashfs.sh
```

---

## Bugs corrigés vs original

| Bug | Fix |
|---|---|
| `async-trait` absent de Cargo.toml | Ajouté |
| `mcp/mod.rs` et `vm/mod.rs` manquants | Créés |
| Race condition `roblox_proxy.rs` (double ownership stdin/stdout) | Architecture canal unique (mpsc sérialisé) |
| `gvproxy` args incorrects | Syntaxe `-listen vsock://:1024 -forward-sock ... -forward-dest ...` |
| `tauri.conf.json` vide | Complet avec windows, bundle, security |
| Préfixe `VIBESTARTER_` → `ROBOLAUNCH_` | Les deux préfixes supportés pour compat |

---

## Nouvelles fonctionnalités vs VibeStarter

- `/mcp/notify` — endpoint push notifications agent → host
- Toolbox API (`/mcp/rpc` → `robolaunch/listToolbox`)
- Tab labels personnalisables par agent
- Frontend : tabs dynamiques, terminal par agent, statut VM live, dark UI améliorée

---

## Clés API

Les clés ne sont **jamais** hardcodées. Configure-les au lancement de chaque agent :

- **Claude** : `ANTHROPIC_API_KEY` dans l'env de la VM
- **Codex** : `OPENAI_API_KEY`
- **Agy** : `CODEIUM_API_KEY` ou via auth `agy login`
- **Aider** : `OPENAI_API_KEY` (ou tout modèle compatible)

Passe-les via la config agent dans l'UI, ou injecte-les dans le squashfs guest.
