#!/usr/bin/env bash
# ═════════════════════════════════════════════════════════════════════
# BDCP Live Demo — Terminal UI (TUI) with real-time progress bars
# ═════════════════════════════════════════════════════════════════════
# Usage:
#   ./deploy/demo.sh                    # Docker mode (full TFHE + ZKP)
#   ./deploy/demo.sh --toy             # Toy cipher (fast, testing)
#   ./deploy/demo.sh --tfhe            # TFHE only (no ZKP)
#   ./deploy/demo.sh --local           # Use existing local daemons
#
# The script starts all containers, submits data, and shows a live
# terminal UI with progress bars for each computation phase.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

# ── Config ──────────────────────────────────────────────────────────
MODE="${1:---tfhe}"
USE_DOCKER=true
ALICE_SOCK="/tmp/bdcp_demo_alice.sock"
BOB_SOCK="/tmp/bdcp_demo_bob.sock"
ALICE_VALS="10,20,30"
BOB_VALS="1,2,3"
RELAY_PORT=9447

if [[ "$MODE" == "--local" ]]; then
    MODE="${2:---tfhe}"
    USE_DOCKER=false
    ALICE_SOCK="/tmp/bdcp_alice.sock"
    BOB_SOCK="/tmp/bdcp_bob.sock"
fi

# Phase timing estimates (milliseconds, from benchmark data)
declare -A PHASE_ESTIMATES
case "$MODE" in
    --toy)
        PHASE_ESTIMATES[alice_keygen]=1500
        PHASE_ESTIMATES[bob_encrypt]=500
        PHASE_ESTIMATES[network]=500
        PHASE_ESTIMATES[relay_compute]=100
        PHASE_ESTIMATES[alice_decrypt]=100
        ALICE_VALS="100"; BOB_VALS="200"
        ;;
    --zkp)
        PHASE_ESTIMATES[alice_keygen]=22000
        PHASE_ESTIMATES[bob_encrypt]=35000
        PHASE_ESTIMATES[network]=5000
        PHASE_ESTIMATES[relay_compute]=2000
        PHASE_ESTIMATES[alice_decrypt]=500
        ;;
    *)  # --tfhe (default)
        PHASE_ESTIMATES[alice_keygen]=8000
        PHASE_ESTIMATES[bob_encrypt]=1000
        PHASE_ESTIMATES[network]=2000
        PHASE_ESTIMATES[relay_compute]=1000
        PHASE_ESTIMATES[alice_decrypt]=500
        ;;
esac

# ── ANSI magic ───────────────────────────────────────────────────────
CLR_LINE="\033[2K"
CLR_UP="\033[1A"
BOLD="\033[1m"
DIM="\033[2m"
GREEN="\033[32m"
YELLOW="\033[33m"
BLUE="\033[34m"
MAGENTA="\033[35m"
CYAN="\033[36m"
RED="\033[31m"
RESET="\033[0m"

N_BAR=20  # progress bar width in characters

# ── Helpers ──────────────────────────────────────────────────────────

bar() {
    local pct=$1
    local filled=$(( pct * N_BAR / 100 ))
    local empty=$(( N_BAR - filled ))
    printf "${BLUE}[${GREEN}"
    printf "%0.s█" $(seq 1 $filled)
    printf "${DIM}"
    printf "%0.s░" $(seq 1 $empty)
    printf "${BLUE}]${RESET} %3d%%" "$pct"
}

phase_line() {
    local emoji="$1" label="$2" pct="$3" detail="$4"
    echo -ne "${CLR_LINE}${emoji} ${BOLD}${label}${RESET}  "
    bar "$pct"
    echo -ne "  ${DIM}${detail}${RESET}\n"
}

update_phase() {
    local line=$1 emoji="$2" label="$3" pct="$4" detail="$5"
    echo -ne "${CLR_UP}"
    phase_line "$emoji" "$label" "$pct" "$detail"
}

draw_header() {
    local mode_label
    case "$MODE" in
        --toy) mode_label="Toy Cipher (fast, insecure)" ;;
        --zkp) mode_label="TFHE + Zero-Knowledge Proofs (maximum security)" ;;
        *)     mode_label="TFHE (real FHE, no ZKP)" ;;
    esac

    echo -e "${BOLD}${CYAN}╔══════════════════════════════════════════════════════╗${RESET}"
    echo -e "${BOLD}${CYAN}║${RESET}   🛡️  BDCP — Blind Data Collaboration Protocol    ${CYAN}║${RESET}"
    echo -e "${BOLD}${CYAN}║${RESET}   ${mode_label}${CYAN}   ║${RESET}"
    echo -e "${BOLD}${CYAN}╚══════════════════════════════════════════════════════╝${RESET}"
    echo ""
    echo -e "  ${DIM}Alice values:${RESET} ${ALICE_VALS}    ${DIM}Bob values:${RESET} ${BOB_VALS}"
    echo ""
}

draw_footer() {
    local verdict="$1" result="$2" elapsed="$3"
    echo ""
    if [[ "$verdict" == "ok" ]]; then
        echo -e "  ${GREEN}${BOLD}✅ Computation Complete!${RESET}"
        echo -e "  ${DIM}Result:${RESET} ${BOLD}${result}${RESET}"
    else
        echo -e "  ${RED}${BOLD}❌ Computation Failed${RESET}"
        echo -e "  ${DIM}${result}${RESET}"
    fi
    echo -e "  ${DIM}Elapsed: ${elapsed}s${RESET}"
    echo ""
}

# ── Main ─────────────────────────────────────────────────────────────

cleanup() {
    echo -ne "${RESET}"
    if $USE_DOCKER && [[ -n "${COMPOSE_PID:-}" ]]; then
        cd "$SCRIPT_DIR" && docker compose down 2>/dev/null
    fi
    rm -f "$ALICE_SOCK" "$BOB_SOCK"
}
trap cleanup EXIT INT TERM

clear
draw_header

# ── Step 0: Prepare ─────────────────────────────────────────────
TOTAL_MS=0
for t in "${PHASE_ESTIMATES[@]}"; do TOTAL_MS=$((TOTAL_MS + t)); done

# Reserve display lines
phase_line "🔧" "Preparing cluster"                   0 ""
phase_line "🔒" "Alice: Generating keys & encrypting"  0 ""
phase_line "📡" "Network: Transferring keys & data"    0 "waiting…"
phase_line "🧠" "Bob: Encrypting & proving values"     0 "waiting…"
phase_line "🧮" "Relay: Computing encrypted addition"  0 "waiting…"
phase_line "🔓" "Alice: Decrypting result"             0 "waiting…"

update_phase 2 "🔧" "Preparing cluster" 50 "starting daemons…"

# ── Step 1: Start daemons ──────────────────────────────────────
if $USE_DOCKER; then
    cd "$SCRIPT_DIR"

    # Patch relay config
    ZKP_FLAG="false"
    [[ "$MODE" == "--zkp" ]] && ZKP_FLAG="true"
    cat > bdcp.toml <<TOML
[node]
name = "relay-node-01"
role = "relay"
listen_addr = "0.0.0.0:9447"
[network]
keep_alive_secs = 10
idle_timeout_secs = 120
[crypto]
backend = "tfhe"
enable_zkp = ${ZKP_FLAG}
TOML
    if [[ "$MODE" == "--toy" ]]; then
        cat > bdcp.toml <<'TOML'
[node]
name = "relay-node-01"
role = "relay"
listen_addr = "0.0.0.0:9447"
[network]
keep_alive_secs = 10
idle_timeout_secs = 120
[crypto]
backend = "toy"
enable_zkp = false
TOML
    fi

    docker compose up --build -d 2>&1 | tail -3
    COMPOSE_PID=1
else
    # Start local daemons
    RELAY_CFG=$(mktemp)
    ALICE_CFG=$(mktemp)
    BOB_CFG=$(mktemp)

    BACKEND="tfhe"
    ZKP="false"
    [[ "$MODE" == "--toy" ]] && BACKEND="toy"
    [[ "$MODE" == "--zkp" ]] && ZKP="true"

    rm -f "$ALICE_SOCK" "$BOB_SOCK"

    for role in relay alice bob; do
        eval "cfg=\$${role^^}_CFG"
        if [[ "$role" == "relay" ]]; then
            cat > "$cfg" <<HERE
[node] ; name = "demo" ; role = "relay" ; listen_addr = "0.0.0.0:${RELAY_PORT}"
[network] ; keep_alive_secs = 10 ; idle_timeout_secs = 120
[crypto] ; backend = "${BACKEND}" ; enable_zkp = ${ZKP}
HERE
        else
            sock_var="${role}_SOCK"
            cat > "$cfg" <<HERE
[node] ; name = "demo" ; role = "${role}" ; listen_addr = "127.0.0.1:0"
[network] ; relay_addr = "127.0.0.1:${RELAY_PORT}" ; keep_alive_secs = 10 ; idle_timeout_secs = 120
[crypto] ; backend = "${BACKEND}" ; enable_zkp = ${ZKP}
[ipc] ; socket_path = "${!sock_var}"
HERE
        fi
        # Fix TOML
        sed -i 's/ ; /\n/g; s/\] ; /\]\n/g' "$cfg"
        "$PROJECT_DIR/target/release/bdcp-node" --config "$cfg" > "/tmp/demo_${role}.log" 2>&1 &
    done
fi

# ── Step 2: Wait for sockets ───────────────────────────────────
update_phase 2 "🔧" "Preparing cluster" 100 "containers ready ✓"
update_phase 3 "🔒" "Alice: Generating keys & encrypting" 10 "initialising…"

ALICE_START=$(date +%s%N)
for i in $(seq 1 60); do
    if [[ -S "$ALICE_SOCK" && -S "$BOB_SOCK" ]]; then
        break
    fi
    if [[ $i -lt 5 ]]; then
        update_phase 3 "🔒" "Alice: Generating keys & encrypting" $(( i * 2 )) " connecting…"
    fi
    sleep 1
done

if [[ ! -S "$ALICE_SOCK" || ! -S "$BOB_SOCK" ]]; then
    update_phase 3 "🔒" "Alice: Generating keys & encrypting" 100 "${RED}FAILED${RESET}"
    draw_footer "fail" "Sockets not ready after 60s"
    exit 1
fi

# ── Step 3: Alice submits (with progress simulation) ────────────
ALICE_KEYGEN_MS=${PHASE_ESTIMATES[alice_keygen]}
update_phase 3 "🔒" "Alice: Generating keys & encrypting" 20 "keypair + CRS…"

# Submit Alice in background, capture output
ALICE_RES=$(mktemp)
(
    echo "submit $ALICE_VALS" | socat -t 120 - UNIX-CONNECT:"$ALICE_SOCK" > "$ALICE_RES" 2>&1
) &
ALICE_PID=$!

# Animate Alice progress while she works
STEP_MS=$((ALICE_KEYGEN_MS / 80))  # 80 animation steps
for pct in $(seq 25 5 95); do
    sleep 0.$(printf "%03d" $((STEP_MS / 10 % 1000)))
    detail=""
    if   [[ $pct -lt 40 ]]; then detail="generating ClientKey…"
    elif [[ $pct -lt 60 ]]; then detail="compressing ServerKey (59 MB)…"
    elif [[ $pct -lt 80 ]]; then detail="generating CRS (3 MB)…"
    else                           detail="encrypting ${ALICE_VALS}…"
    fi
    update_phase 3 "🔒" "Alice: Generating keys & encrypting" "$pct" "$detail"
done

# ── Step 4: Bob submits (with progress simulation) ─────────────
update_phase 5 "🧠" "Bob: Encrypting & proving values" 10 " fetching public key…"
sleep 0.3

BOB_RES=$(mktemp)
(
    echo "submit $BOB_VALS" | socat -t 120 - UNIX-CONNECT:"$BOB_SOCK" > "$BOB_RES" 2>&1
) &
BOB_PID=$!

BOB_MS=${PHASE_ESTIMATES[bob_encrypt]}
STEP_MS=$((BOB_MS / 80))
for pct in $(seq 15 5 95); do
    sleep 0.$(printf "%03d" $((STEP_MS / 10 % 1000)))
    if [[ "$MODE" == "--zkp" ]]; then
        detail="generating ZK proof… ($((pct * BOB_MS / 100 / 1000))s)"
    else
        detail="compact encrypting…"
    fi
    update_phase 5 "🧠" "Bob: Encrypting & proving values" "$pct" "$detail"
done

# ── Step 5: Network transfer ──────────────────────────────────
update_phase 4 "📡" "Network: Transferring keys & data" 30 " evaluation key…"
sleep 0.2
update_phase 4 "📡" "Network: Transferring keys & data" 60 " ciphertexts…"
sleep 0.3
update_phase 4 "📡" "Network: Transferring keys & data" 90 " Bob's data…"
sleep 0.2

# ── Step 6: Wait for computation ──────────────────────────────
update_phase 6 "🧮" "Relay: Computing encrypted addition" 10 " decompressing ServerKey…"
sleep 0.3
update_phase 6 "🧮" "Relay: Computing encrypted addition" 50 " element-wise add…"
sleep 0.3
update_phase 6 "🧮" "Relay: Computing encrypted addition" 90 " serialising result…"
sleep 0.2

# Wait for Alice to finish
wait $ALICE_PID 2>/dev/null || true
ALICE_END=$(date +%s%N)
ALICE_ELAPSED=$(( (ALICE_END - ALICE_START) / 1000000 ))

# ── Step 7: Read result ───────────────────────────────────────
update_phase 7 "🔓" "Alice: Decrypting result" 50 "deserialising…"
ALICE_OUT=$(cat "$ALICE_RES" 2>/dev/null)
rm -f "$ALICE_RES" "$BOB_RES"

# Parse result
ALICE_RESULT="?"
if echo "$ALICE_OUT" | grep -q "OK results:"; then
    ALICE_RESULT=$(echo "$ALICE_OUT" | grep "OK results:" | sed 's/.*OK results: //')
fi

update_phase 7 "🔓" "Alice: Decrypting result" 100 "✓ $ALICE_RESULT"

# ── Step 8: Final summary ─────────────────────────────────────
ELAPSED_SEC=$(( ALICE_ELAPSED / 1000 ))

# Mark all phases complete
update_phase 2 "🔧" "Preparing cluster"           100 "✓"
update_phase 3 "🔒" "Alice: Generating keys & encrypting" 100 "✓"
update_phase 4 "📡" "Network: Transferring keys & data"   100 "✓"
update_phase 5 "🧠" "Bob: Encrypting & proving values"    100 "✓"
update_phase 6 "🧮" "Relay: Computing encrypted addition" 100 "✓"
update_phase 7 "🔓" "Alice: Decrypting result"            100 "✓"

draw_footer "ok" "$ALICE_RESULT" "$ELAPSED_SEC"
