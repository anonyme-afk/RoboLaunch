#!/bin/sh
# PATH layering for the embedded guest.
#
# /home/app-user is on the qcow2 user volume (see /etc/fstab), so
# anything under $HOME persists across VM restarts. Base-tier CLIs
# shipped in the squashfs live under /opt/ instead, because anything
# that would otherwise land under /home/app-user gets hidden once
# the qcow2 mounts over it.
#
# Order, highest priority first:
#   1. $HOME/.rokit/bin            -- per-project wally/rojo/… shims
#                                     installed by `rokit install`
#   2. $HOME/.local/bin            -- Claude + Antigravity user-tier
#                                     (qcow2, both installers drop here)
#   3. $HOME/npm-global/bin        -- Codex user-tier (qcow2)
#   4. /opt/claude-base/.local/bin -- Claude base-tier (squashfs)
#   5. /opt/agy-base/.local/bin    -- Antigravity base-tier (squashfs)
#   6. /opt/npm-global/bin         -- Codex base-tier (squashfs)
#   7. system PATH                 -- /usr/local/bin … /usr/bin etc.
#
# "Update CLIs" writes new Claude into $HOME/.local/bin (via
# HOME=$HOME install.sh), new Antigravity into the same dir (its
# installer also targets ~/.local/bin), and new Codex into
# $HOME/npm-global (via NPM_CONFIG_PREFIX below). "Reset CLI
# overrides" wipes those $HOME subtrees; the next shell falls
# through to the /opt base tier.
#
# `$HOME/.rokit/bin` comes first so a project-pinned rojo/wally
# (installed by `rokit install` from rokit.toml) wins over the
# fallback `/usr/local/bin/rojo` we ship in the squashfs.

ROBOLAUNCH_USER_BIN="${HOME}/.rokit/bin:${HOME}/.local/bin:${HOME}/npm-global/bin"
ROBOLAUNCH_BASE_BIN="/opt/claude-base/.local/bin:/opt/agy-base/.local/bin:/opt/npm-global/bin"

case ":$PATH:" in
    *":${HOME}/.rokit/bin:"*) ;;
    *)
        export PATH="$ROBOLAUNCH_USER_BIN:$ROBOLAUNCH_BASE_BIN:$PATH"
        ;;
esac

# Point npm at the user-tier global prefix by default so `npm install
# -g foo` lands under $HOME/npm-global without needing --prefix.
export NPM_CONFIG_PREFIX="${HOME}/npm-global"

# npm's default cache is $HOME/.npm — already on the qcow2 volume.
# Declaring it here keeps the path explicit and makes it trivial to
# retarget if we ever need a different location.
export NPM_CONFIG_CACHE="${HOME}/.npm"

# Advertise 24-bit truecolor to CLIs running inside the guest.
# `request_pty` sets TERM=xterm-256color (256 colors); CLIs that use
# `supports-color` (agy, claude, codex, chalk-based tools) gate
# truecolor on COLORTERM, so without this they fall back to 256.
# xterm.js on the host renders 24-bit escapes natively.
export COLORTERM=truecolor
