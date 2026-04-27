#!/usr/bin/env bash
# cross-vendor-demo.sh — boot healthkit + mock-weather servers, prove
# they're discoverable side-by-side from one ATD client.
#
# Usage:
#   ./scripts/cross-vendor-demo.sh up        # boot both, run discover, print bridge cmds (default)
#   ./scripts/cross-vendor-demo.sh down      # tear down both
#   ./scripts/cross-vendor-demo.sh status    # show what's running
#
# Optional env vars (override defaults):
#   ATD_REPO     atd-mvp repo                          (default: $HOME/proj/atd-mvp)
#   HK_REPO      healthkit_cli repo                    (default: $HOME/proj/healthkit_cli)
#   HK_SOCK      healthkit socket                      (default: /tmp/hk.sock)
#   WX_SOCK      mock-weather socket                   (default: /tmp/atd-weather.sock)
#
# Prereqs:
#   - atd-mvp built:        (cd $ATD_REPO && cargo build --release \
#                              -p atd-mock-weather-server -p atd-cli)
#   - healthkit_cli built:  (cd $HK_REPO && cargo build --release)
#   - healthkit auth'd:     $HK_REPO/target/release/healthkit auth login
#
# See docs/integrations/cross-vendor-pattern.md for the design rationale.

set -euo pipefail

ACTION="${1:-up}"

ATD_REPO="${ATD_REPO:-$HOME/proj/atd-mvp}"
HK_REPO="${HK_REPO:-$HOME/proj/healthkit_cli}"
HK_SOCK="${HK_SOCK:-/tmp/hk.sock}"
WX_SOCK="${WX_SOCK:-/tmp/atd-weather.sock}"

ATD_BIN="$ATD_REPO/target/release/atd"
WX_BIN="$ATD_REPO/target/release/atd-mock-weather-server"
HK_BIN="$HK_REPO/target/release/healthkit"

HK_PIDFILE=/tmp/hk-cross-demo.pid
WX_PIDFILE=/tmp/wx-cross-demo.pid

c_grn='\033[0;32m'; c_yel='\033[0;33m'; c_cyn='\033[0;36m'; c_rst='\033[0m'
ok()    { printf "${c_grn}✓${c_rst} %s\n" "$*"; }
info()  { printf "${c_cyn}→${c_rst} %s\n" "$*"; }
warn()  { printf "${c_yel}!${c_rst} %s\n" "$*"; }

is_running() {
    [[ -f "$1" ]] && kill -0 "$(<"$1")" 2>/dev/null
}

# ---------- preflight ----------
preflight() {
    [[ -x "$ATD_BIN" ]] || { echo "missing: $ATD_BIN — build with: cargo build --release -p atd-cli" >&2; exit 1; }
    [[ -x "$WX_BIN"  ]] || { echo "missing: $WX_BIN — build with: cargo build --release -p atd-mock-weather-server" >&2; exit 1; }
    [[ -x "$HK_BIN"  ]] || { echo "missing: $HK_BIN — build with: cd $HK_REPO && cargo build --release" >&2; exit 1; }

    local s
    s=$("$HK_BIN" auth status 2>&1 || true)
    if echo "$s" | grep -q '"method": "none"'; then
        warn "no OAuth token saved; healthkit live calls will fail"
        warn "run:  $HK_BIN auth login"
        warn "(skills.list/get and tool_list still work without auth)"
    fi
    ok "preflight: 3 binaries present"
}

# ---------- up ----------
up() {
    preflight

    # mock-weather
    if is_running "$WX_PIDFILE"; then
        warn "mock-weather already running (pid $(<"$WX_PIDFILE")); skipping"
    else
        info "starting atd-mock-weather-server on $WX_SOCK ..."
        rm -f "$WX_SOCK"
        nohup "$WX_BIN" --sock "$WX_SOCK" > /tmp/wx-cross-demo.log 2>&1 &
        echo $! > "$WX_PIDFILE"
        local d=$((SECONDS + 5))
        while [[ ! -S "$WX_SOCK" ]]; do
            [[ $SECONDS -ge $d ]] && { cat /tmp/wx-cross-demo.log; exit 1; }
            sleep 0.1
        done
        ok "mock-weather up (pid $(<"$WX_PIDFILE"), sock $WX_SOCK)"
    fi

    # healthkit
    if is_running "$HK_PIDFILE"; then
        warn "healthkit already running (pid $(<"$HK_PIDFILE")); skipping"
    else
        info "starting healthkit serve on $HK_SOCK ..."
        rm -f "$HK_SOCK"
        nohup "$HK_BIN" serve \
            --sock "$HK_SOCK" \
            --grant-capability healthkit:read \
            --grant-capability healthkit:write \
            > /tmp/hk-cross-demo.log 2>&1 &
        echo $! > "$HK_PIDFILE"
        local d=$((SECONDS + 8))
        while [[ ! -S "$HK_SOCK" ]]; do
            [[ $SECONDS -ge $d ]] && { cat /tmp/hk-cross-demo.log; exit 1; }
            sleep 0.2
        done
        ok "healthkit up (pid $(<"$HK_PIDFILE"), sock $HK_SOCK)"
    fi

    # discover side-by-side
    echo
    info "═══ tools published by mock-weather ═══"
    "$ATD_BIN" --sock "$WX_SOCK" list 2>&1 | sed 's/^/  /'

    echo
    info "═══ tools published by healthkit (first 6 + total) ═══"
    "$ATD_BIN" --sock "$HK_SOCK" list 2>&1 | head -7 | sed 's/^/  /'
    "$ATD_BIN" --sock "$HK_SOCK" list 2>&1 | tail -1 | sed 's/^/  /'

    # bridge registration help
    echo
    info "═══ to wire BOTH into Hermes (one agent session, both vendors) ═══"
    echo "  hermes mcp add weather --command $ATD_REPO/target/release/atd-mcp-bridge \\"
    echo "    --env ATD_SOCK=$WX_SOCK"
    echo "  hermes mcp add healthkit --command $ATD_REPO/target/release/atd-mcp-bridge \\"
    echo "    --env ATD_SOCK=$HK_SOCK ATD_REQUEST_CAPS=healthkit:read,healthkit:write"
    echo
    info "═══ to wire BOTH into Claude Code ═══"
    echo "  claude mcp add -s user --env=ATD_SOCK=$WX_SOCK \\"
    echo "    weather $ATD_REPO/target/release/atd-mcp-bridge"
    echo "  claude mcp add -s user --env=ATD_SOCK=$HK_SOCK \\"
    echo "    --env=ATD_REQUEST_CAPS=healthkit:read,healthkit:write \\"
    echo "    healthkit $ATD_REPO/target/release/atd-mcp-bridge"

    echo
    ok "demo ready. Sample prompt for the agent:"
    echo "    \"我跑 5km 应该穿什么？\""
    echo
    echo "    Agent should call:"
    echo "      mock:weather.summary             (one-line outdoor gloss)"
    echo "      huawei:hms.healthkit.heartrate   (recent HR readings)"
    echo "      huawei:hms.healthkit.sleep       (last night's sleep)"
    echo "    …and compose a recommendation across both vendors' data."
    echo
    info "tear down: $0 down"
}

# ---------- down ----------
down() {
    for pf in "$WX_PIDFILE" "$HK_PIDFILE"; do
        if is_running "$pf"; then
            local pid=$(<"$pf")
            info "stopping (pid $pid) ..."
            kill "$pid" 2>/dev/null || true
            sleep 0.3
            kill -9 "$pid" 2>/dev/null || true
            rm -f "$pf"
        fi
    done
    rm -f "$WX_SOCK" "$HK_SOCK"
    ok "torn down"
}

# ---------- status ----------
status() {
    is_running "$WX_PIDFILE" && ok "mock-weather running (pid $(<"$WX_PIDFILE"), sock $WX_SOCK)" || info "mock-weather NOT running"
    is_running "$HK_PIDFILE" && ok "healthkit running (pid $(<"$HK_PIDFILE"), sock $HK_SOCK)" || info "healthkit NOT running"
}

case "$ACTION" in
    up)     up ;;
    down)   down ;;
    status) status ;;
    *) echo "usage: $0 {up|down|status}" >&2; exit 1 ;;
esac
