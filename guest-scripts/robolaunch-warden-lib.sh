#!/usr/bin/env bash
# Shared helpers for `robolaunch-warden`.
#
# Keep this file side-effect-light: the main daemon owns policy
# constants and loop state; this file owns cgroup/PSI primitives.

read_psi_some_file() {
    local file=$1 result
    result=$(awk -F'[= ]' '/^some/{printf "%d", $3; exit}' "$file" 2>/dev/null) || true
    if [ -n "$result" ]; then
        echo "$result"
        return 0
    fi
    return 1
}

read_cpu_psi_some() {
    read_psi_some_file "$SLICE/cpu.pressure" \
        || read_psi_some_file /proc/pressure/cpu \
        || echo 0
}

read_io_psi_some() {
    read_psi_some_file "$SLICE/io.pressure" \
        || read_psi_some_file /proc/pressure/io \
        || echo 0
}

read_mem_psi_some() {
    read_psi_some_file "$SLICE/memory.pressure" \
        || read_psi_some_file /proc/pressure/memory \
        || echo 0
}

add_trigger_reason() {
    if [ -n "$trigger_reasons" ]; then
        trigger_reasons="${trigger_reasons}; $1"
    else
        trigger_reasons="$1"
    fi
}

update_psi_trigger() {
    local name=$1 value=$2 sustained_count=$3
    if (( value >= PSI_SPIKE_THRESHOLD )); then
        add_trigger_reason "$name spike (${value}% >= ${PSI_SPIKE_THRESHOLD}%)"
        psi_next_count=0
        return
    fi
    if (( sustained_count >= SUSTAINED_PSI_SECONDS )); then
        add_trigger_reason "$name sustained (${value}% for ${sustained_count}s >= ${SUSTAINED_PSI_THRESHOLD}%/${SUSTAINED_PSI_SECONDS}s)"
        psi_next_count=0
        return
    fi
    # psi_next_count est une variable globale lue par robolaunch-warden
    # après chaque appel. SC2034 est un faux positif ici.
    # shellcheck disable=SC2034
    psi_next_count=$sustained_count
}

count_tcp_established() {
    ss -tn state established 2>/dev/null | tail -n +2 | wc -l
}

log_tcp_pressure_detail() {
    log "network detail top_peers:"
    ss -Htn state established 2>/dev/null \
        | awk '{peer=$4; sub(/:[0-9]+$/, "", peer); print peer}' \
        | sort | uniq -c | sort -nr | head -10 \
        | while IFS= read -r line; do
            [ -n "$line" ] && log "network detail peer $line"
        done

    log "network detail top_processes:"
    ss -Htnp state established 2>/dev/null \
        | awk '{peer=$4; sub(/:[0-9]+$/, "", peer); print peer, $NF}' \
        | sort | uniq -c | sort -nr | head -20 \
        | while IFS= read -r line; do
            [ -n "$line" ] && log "network detail process $line"
        done

    log "network detail top_agents:"
    ss -Htnp state established 2>/dev/null \
        | sed -n 's/.*pid=\([0-9][0-9]*\).*/\1/p' \
        | while IFS= read -r pid; do
            [ -n "$pid" ] || continue
            cg=$(grep -m1 'robolaunch-agent-.*\.scope' "/proc/$pid/cgroup" 2>/dev/null || true)
            [ -n "$cg" ] || continue
            agent=${cg##*robolaunch-agent-}
            agent=${agent%%.scope*}
            comm=$(cat "/proc/$pid/comm" 2>/dev/null || echo unknown)
            printf '%s %s\n' "$agent" "$comm"
        done \
        | sort | uniq -c | sort -nr | head -20 \
        | while IFS= read -r line; do
            [ -n "$line" ] && log "network detail agent $line"
        done
}

read_oom_kill_count() {
    local count
    count=$(awk '/^oom_kill[ \t]/{print $2; exit}' "$SLICE/memory.events" 2>/dev/null) || true
    echo "${count:-0}"
}

probe_psi_sources() {
    if [ -r "$SLICE/cpu.pressure" ]; then
        local v
        v=$(read_psi_some_file "$SLICE/cpu.pressure" || true)
        log "PSI source cgroup ($SLICE/cpu.pressure): readable, current some=${v:-empty}"
    else
        log "PSI source cgroup ($SLICE/cpu.pressure): NOT readable"
    fi
    if [ -r /proc/pressure/cpu ]; then
        local v
        v=$(read_psi_some_file /proc/pressure/cpu || true)
        log "PSI source system (/proc/pressure/cpu): readable, current some=${v:-empty}"
    else
        log "PSI source system (/proc/pressure/cpu): NOT readable - CPU trigger DISABLED"
    fi
    if [ -r "$SLICE/io.pressure" ]; then
        local v
        v=$(read_psi_some_file "$SLICE/io.pressure" || true)
        log "PSI source cgroup ($SLICE/io.pressure): readable, current some=${v:-empty}"
    else
        log "PSI source cgroup ($SLICE/io.pressure): NOT readable"
    fi
    if [ -r /proc/pressure/io ]; then
        local v
        v=$(read_psi_some_file /proc/pressure/io || true)
        log "PSI source system (/proc/pressure/io): readable, current some=${v:-empty}"
    else
        log "PSI source system (/proc/pressure/io): NOT readable"
    fi
    if [ -r "$SLICE/memory.pressure" ]; then
        local v
        v=$(read_psi_some_file "$SLICE/memory.pressure" || true)
        log "PSI source cgroup ($SLICE/memory.pressure): readable, current some=${v:-empty}"
    else
        log "PSI source cgroup ($SLICE/memory.pressure): NOT readable"
    fi
    if [ -r /proc/pressure/memory ]; then
        local v
        v=$(read_psi_some_file /proc/pressure/memory || true)
        log "PSI source system (/proc/pressure/memory): readable, current some=${v:-empty}"
    else
        log "PSI source system (/proc/pressure/memory): NOT readable"
    fi
}

is_frozen() {
    local d=$1
    local v
    v=$(awk '/^frozen[ \t]/{print $2; exit}' "$d/cgroup.events" 2>/dev/null)
    [ "$v" = "1" ]
}

count_awake() {
    local count=0 d
    for d in "$SLICE"/robolaunch-agent-*.scope; do
        [ -d "$d" ] || continue
        is_frozen "$d" && continue
        count=$(( count + 1 ))
    done
    echo "$count"
}

write_reason() {
    local agent_id=$1
    local reason=$2
    echo "$reason" > "$STATE_DIR/$agent_id.reason" 2>/dev/null || true
}

freeze_specific() {
    local scope_dir=$1
    local reason=$2

    [ -d "$scope_dir" ] || return 1
    is_frozen "$scope_dir" && return 0

    local unit_name agent_id
    unit_name=$(basename "$scope_dir")
    agent_id=${unit_name#robolaunch-agent-}
    agent_id=${agent_id%.scope}

    local ok=0
    if echo 1 > "$scope_dir/cgroup.freeze" 2>/dev/null; then
        ok=1
    elif systemctl freeze "$unit_name" 2>/dev/null; then
        ok=1
    fi
    if (( ok == 0 )); then
        log "ERROR: freeze refused for $agent_id"
        return 1
    fi

    sleep 0.05
    if is_frozen "$scope_dir"; then
        write_reason "$agent_id" "$reason"
        log "FREEZE agent_id=$agent_id reason=$reason (verified)"
        return 0
    fi
    log "ERROR: freeze for $agent_id did not take (cgroup.events says unfrozen)"
    return 1
}

freeze_heaviest_n() {
    local reason=$1
    local count=${2:-1}
    local frozen=0 scope_dir d mem

    if ! [[ "$count" =~ ^[0-9]+$ ]]; then
        count=1
    fi
    if (( count < 1 )); then
        count=1
    elif (( count > 16 )); then
        count=16
    fi

    while IFS= read -r scope_dir; do
        if [ -z "$scope_dir" ]; then
            continue
        fi
        if freeze_specific "$scope_dir" "$reason"; then
            frozen=$(( frozen + 1 ))
        fi
    done < <(
        for d in "$SLICE"/robolaunch-agent-*.scope; do
            [ -d "$d" ] || continue
            is_frozen "$d" && continue
            mem=$(cat "$d/memory.current" 2>/dev/null || echo 0)
            printf '%s\t%s\n' "$mem" "$d"
        done | sort -rn | head -n "$count" | cut -f2-
    )

    if (( frozen < count )); then
        log "freeze batch requested=$count frozen=$frozen"
    fi

    (( frozen > 0 ))
}

cleanup_stale_state() {
    local f name agent_id scope_dir
    for f in "$STATE_DIR"/*.reason; do
        [ -f "$f" ] || continue
        name=$(basename "$f")
        agent_id=${name%.reason}
        scope_dir="$SLICE/robolaunch-agent-$agent_id.scope"
        if [ ! -d "$scope_dir" ] || ! is_frozen "$scope_dir"; then
            rm -f "$f"
        fi
    done
}
