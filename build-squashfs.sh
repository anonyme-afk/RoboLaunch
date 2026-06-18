#!/bin/bash
# build-squashfs.sh — Reconstruit l'image squashfs invité RoboLaunch
#
# À lancer sous Linux (ou WSL). Ce script injecte les scripts guest-scripts/
# de RoboLaunch dans une copie de l'image de base (à l'origine celle de
# VibeStarter), en remplaçant les anciens scripts vibestarter-* par les
# nouveaux robolaunch-*.
#
# Usage:
#   ./build-squashfs.sh /chemin/vers/image-de-base.squashfs
#
# Sortie: robolaunch-guest-rebuilt.squashfs
#         (à renommer en robolaunch-guest.squashfs et à copier dans
#          src-tauri/resources/vm/)

set -euo pipefail

ORIGINAL="${1:-vibestarter-guest.squashfs}"
WORKDIR="$(mktemp -d)"
SCRIPTS_DIR="$(cd "$(dirname "$0")" && pwd)/guest-scripts"
OUT="robolaunch-guest-rebuilt.squashfs"

log() { echo "[build-squashfs] $*"; }

cleanup() { rm -rf "$WORKDIR"; }
trap cleanup EXIT

if [ ! -f "$ORIGINAL" ]; then
  echo "Usage: $0 /chemin/vers/image-de-base.squashfs" >&2
  echo "(l'image de base d'origine, ex: vibestarter-guest.squashfs)" >&2
  exit 1
fi

command -v unsquashfs >/dev/null || { echo "Installe squashfs-tools: sudo apt install squashfs-tools"; exit 1; }
command -v mksquashfs >/dev/null || { echo "Installe squashfs-tools: sudo apt install squashfs-tools"; exit 1; }

log "Extraction de $ORIGINAL..."
unsquashfs -d "$WORKDIR/root" "$ORIGINAL"

log "Installation des scripts RoboLaunch..."

# Scripts sbin (daemons système + scripts de boot)
SBIN="$WORKDIR/root/usr/local/sbin"
mkdir -p "$SBIN"
for f in \
  robolaunch-warden \
  robolaunch-net-up \
  robolaunch-authkeys \
  robolaunch-ssh-keygen \
  robolaunch-zram \
  robolaunch-user-volume-init \
  robolaunch-aider-setup; do
  if [ -f "$SCRIPTS_DIR/$f" ]; then
    install -m 755 "$SCRIPTS_DIR/$f" "$SBIN/$f"
    log "  → /usr/local/sbin/$f"
  else
    log "  ! manquant: $SCRIPTS_DIR/$f (ignoré)"
  fi
done

# lib (helper du warden)
LIB="$WORKDIR/root/usr/local/lib"
mkdir -p "$LIB"
install -m 644 "$SCRIPTS_DIR/robolaunch-warden-lib.sh" "$LIB/robolaunch-warden-lib.sh"
log "  → /usr/local/lib/robolaunch-warden-lib.sh"

# profile.d (PATH pour les agents)
PROFILE="$WORKDIR/root/etc/profile.d"
mkdir -p "$PROFILE"
install -m 644 "$SCRIPTS_DIR/robolaunch-path.sh" "$PROFILE/robolaunch-path.sh"
log "  → /etc/profile.d/robolaunch-path.sh"

# opt/robolaunch (bridges MCP)
OPT="$WORKDIR/root/opt/robolaunch"
mkdir -p "$OPT"
install -m 755 "$SCRIPTS_DIR/mcp-stub.cjs"         "$OPT/mcp-stub.cjs"
install -m 755 "$SCRIPTS_DIR/roblox-mcp-proxy.cjs" "$OPT/roblox-mcp-proxy.cjs"
log "  → /opt/robolaunch/mcp-stub.cjs"
log "  → /opt/robolaunch/roblox-mcp-proxy.cjs"

# squelette home/app-user (le wrapper aider est installé au runtime)
APPUSER="$WORKDIR/root/home/app-user/.local/bin"
mkdir -p "$APPUSER"

# unité systemd du warden
SYSTEMD="$WORKDIR/root/etc/systemd/system"
mkdir -p "$SYSTEMD"

cat > "$SYSTEMD/robolaunch-warden.service" << 'UNIT'
[Unit]
Description=RoboLaunch in-guest memory warden
After=multi-user.target
StartLimitIntervalSec=0

[Service]
Type=simple
ExecStart=/usr/local/sbin/robolaunch-warden
Restart=always
RestartSec=1
StandardOutput=journal
StandardError=journal
Slice=system.slice
CPUWeight=10000
IOWeight=10000
MemoryAccounting=yes
MemoryMin=128M
MemoryLow=128M
OOMScoreAdjust=-1000
Nice=-10
User=root

[Install]
WantedBy=multi-user.target
UNIT
log "  → /etc/systemd/system/robolaunch-warden.service"

log "Reconstruction du squashfs (compression zstd)..."
mksquashfs "$WORKDIR/root" "$OUT" \
  -comp zstd \
  -Xcompression-level 9 \
  -noappend \
  -no-progress

SIZE=$(du -sh "$OUT" | cut -f1)
log "Terminé! Sortie: $OUT ($SIZE)"
log ""
log "Étapes suivantes:"
log "  1. Copie $OUT dans src-tauri/resources/vm/"
log "  2. Renomme-le en robolaunch-guest.squashfs"
log "  3. Build l'app Tauri: cargo tauri build"
