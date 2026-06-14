#!/bin/bash
# build-squashfs.sh — Rebuild vibestarter-guest.squashfs
#
# Run this on Linux (or WSL) to inject the rebuilt guest scripts
# into a copy of the original squashfs.
#
# Usage:
#   ./build-squashfs.sh /path/to/original/vibestarter-guest.squashfs
#
# Output: vibestarter-guest-rebuilt.squashfs (drop it in resources/vm/)

set -euo pipefail

ORIGINAL="${1:-vibestarter-guest.squashfs}"
WORKDIR="$(mktemp -d)"
SCRIPTS_DIR="$(dirname "$0")/guest-scripts"
OUT="vibestarter-guest-rebuilt.squashfs"

log() { echo "[build-squashfs] $*"; }

cleanup() { rm -rf "$WORKDIR"; }
trap cleanup EXIT

if [ ! -f "$ORIGINAL" ]; then
  echo "Usage: $0 /path/to/vibestarter-guest.squashfs" >&2
  exit 1
fi

command -v unsquashfs >/dev/null || { echo "Install squashfs-tools first: sudo apt install squashfs-tools"; exit 1; }
command -v mksquashfs >/dev/null || { echo "Install squashfs-tools first: sudo apt install squashfs-tools"; exit 1; }

log "Extracting $ORIGINAL..."
unsquashfs -d "$WORKDIR/root" "$ORIGINAL"

log "Installing guest scripts..."

# sbin scripts (system daemons + boot scripts)
SBIN="$WORKDIR/root/usr/local/sbin"
mkdir -p "$SBIN"
for f in \
  vibestarter-warden \
  vibestarter-net-up \
  vibestarter-authkeys \
  vibestarter-ssh-keygen \
  vibestarter-zram \
  vibestarter-user-volume-init \
  vibestarter-aider-setup; do
  if [ -f "$SCRIPTS_DIR/$f" ]; then
    install -m 755 "$SCRIPTS_DIR/$f" "$SBIN/$f"
    log "  → /usr/local/sbin/$f"
  fi
done

# lib (warden helper)
LIB="$WORKDIR/root/usr/local/lib"
mkdir -p "$LIB"
install -m 644 "$SCRIPTS_DIR/vibestarter-warden-lib.sh" "$LIB/vibestarter-warden-lib.sh"
log "  → /usr/local/lib/vibestarter-warden-lib.sh"

# profile.d (PATH for agents)
PROFILE="$WORKDIR/root/etc/profile.d"
mkdir -p "$PROFILE"
install -m 644 "$SCRIPTS_DIR/vibestarter-path.sh" "$PROFILE/vibestarter-path.sh"
log "  → /etc/profile.d/vibestarter-path.sh"

# opt/vibestarter (MCP bridges)
OPT="$WORKDIR/root/opt/vibestarter"
mkdir -p "$OPT"
install -m 755 "$SCRIPTS_DIR/mcp-stub.cjs"            "$OPT/mcp-stub.cjs"
install -m 755 "$SCRIPTS_DIR/roblox-mcp-proxy.cjs"    "$OPT/roblox-mcp-proxy.cjs"
log "  → /opt/vibestarter/mcp-stub.cjs"
log "  → /opt/vibestarter/roblox-mcp-proxy.cjs"

# home/app-user skeleton (aider wrapper will be installed at runtime)
APPUSER="$WORKDIR/root/home/app-user/.local/bin"
mkdir -p "$APPUSER"

# systemd units (already in squashfs, but update warden unit just in case)
SYSTEMD="$WORKDIR/root/etc/systemd/system"
mkdir -p "$SYSTEMD"

cat > "$SYSTEMD/vibestarter-warden.service" << 'UNIT'
[Unit]
Description=Vibestarter in-guest memory warden
After=multi-user.target
StartLimitIntervalSec=0

[Service]
Type=simple
ExecStart=/usr/local/sbin/vibestarter-warden
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
log "  → /etc/systemd/system/vibestarter-warden.service"

log "Rebuilding squashfs (zstd compression)..."
mksquashfs "$WORKDIR/root" "$OUT" \
  -comp zstd \
  -Xcompression-level 9 \
  -noappend \
  -no-progress

SIZE=$(du -sh "$OUT" | cut -f1)
log "Done! Output: $OUT ($SIZE)"
log ""
log "Next steps:"
log "  1. Copy $OUT to your Tauri resources/vm/ folder"
log "  2. Rename it to vibestarter-guest.squashfs"
log "  3. Build the Tauri app: cargo tauri build"
