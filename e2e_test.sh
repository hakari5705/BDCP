#!/usr/bin/env bash
set -euo pipefail

# ═══════════════════════════════════════════════════════════════
# BDCP End-to-End Integration Test
# Launches relay, Alice, and Bob daemons; submits values via IPC;
# validates computed result.
# ═══════════════════════════════════════════════════════════════

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
RELAY_LOG=$(mktemp)
ALICE_LOG=$(mktemp)
BOB_LOG=$(mktemp)
PID_FILE=$(mktemp)

ALICE_SOCK="/tmp/bdcp_alice.sock"
BOB_SOCK="/tmp/bdcp_bob.sock"
RELAY_CFG=$(mktemp)
ALICE_CFG=$(mktemp)
BOB_CFG=$(mktemp)

USE_TFHE=false
USE_ZKP=false
CARGO_FLAGS="--release"

cleanup() {
    echo ""
    echo "=== Cleaning up ==="
    if [[ -f "$PID_FILE" ]]; then
        while IFS= read -r pid; do
            kill "$pid" 2>/dev/null || true
        done < "$PID_FILE"
    fi
    rm -f "$RELAY_LOG" "$ALICE_LOG" "$BOB_LOG" "$PID_FILE"
    rm -f "$ALICE_SOCK" "$BOB_SOCK"
    rm -f "$RELAY_CFG" "$ALICE_CFG" "$BOB_CFG"
    rm -f /tmp/bdcp_e2e_alice_out.* /tmp/bdcp_e2e_bob_out.*
    echo "Done."
}
trap cleanup EXIT

# ── Parse args ───────────────────────────────────────────────
while [[ $# -gt 0 ]]; do
    case "$1" in
        --tfhe)     USE_TFHE=true;  CARGO_FLAGS="$CARGO_FLAGS --features tfhe-backend" ;;
        --zkp)      USE_ZKP=true ;;
        --debug)    CARGO_FLAGS="" ;;  # remove --release
        --help|-h)
            echo "Usage: $0 [--tfhe] [--zkp] [--debug]"
            exit 0 ;;
        *) echo "Unknown: $1"; exit 1 ;;
    esac
    shift
done

# Toy cipher supports single value; TFHE supports batches
if "$USE_TFHE"; then
    RELAY_VALUES="10,20,30"
    BOB_VALUES="1,2,3"
    EXPECTED="11, 22, 33"
else
    RELAY_VALUES="100"
    BOB_VALUES="200"
    EXPECTED="300"
fi

echo "═══════════════════════════════════════════════════════"
echo "  BDCP E2E Test — TFHE=${USE_TFHE} ZKP=${USE_ZKP}"
echo "═══════════════════════════════════════════════════════"

# ── Prepare configs ─────────────────────────────────────────
BACKEND=$("$USE_TFHE" && echo "tfhe" || echo "toy")

sed "s|backend = .*|backend = \"$BACKEND\"|; s|enable_zkp = .*|enable_zkp = $([[ $USE_ZKP == true ]] && echo true || echo false)|" \
    "$SCRIPT_DIR/deploy/bdcp.toml" > "$RELAY_CFG"

sed "s|socket_path = .*|socket_path = \"$ALICE_SOCK\"|; \
     s|backend = .*|backend = \"$BACKEND\"|; \
     s|enable_zkp = .*|enable_zkp = $([[ $USE_ZKP == true ]] && echo true || echo false)|" \
    "$SCRIPT_DIR/deploy/alice.toml" > "$ALICE_CFG"

sed "s|socket_path = .*|socket_path = \"$BOB_SOCK\"|; \
     s|backend = .*|backend = \"$BACKEND\"|; \
     s|enable_zkp = .*|enable_zkp = $([[ $USE_ZKP == true ]] && echo true || echo false)|" \
    "$SCRIPT_DIR/deploy/bob.toml" > "$BOB_CFG"

# ── Build ────────────────────────────────────────────────────
echo ""
echo "=== Building bdcp-node ==="
# shellcheck disable=SC2086
cargo build $CARGO_FLAGS -p bdcp-node

# ── Start daemons ───────────────────────────────────────────
echo ""
echo "=== Starting Relay daemon ==="
# shellcheck disable=SC2086
cargo run $CARGO_FLAGS -p bdcp-node -- --config "$RELAY_CFG" \
    > "$RELAY_LOG" 2>&1 &
echo $! >> "$PID_FILE"
echo "  Relay PID: $!"
sleep 1

echo "=== Starting Alice daemon ==="
rm -f "$ALICE_SOCK"
# shellcheck disable=SC2086
cargo run $CARGO_FLAGS -p bdcp-node -- --config "$ALICE_CFG" \
    > "$ALICE_LOG" 2>&1 &
echo $! >> "$PID_FILE"
echo "  Alice PID: $!"

echo "=== Starting Bob daemon ==="
rm -f "$BOB_SOCK"
# shellcheck disable=SC2086
cargo run $CARGO_FLAGS -p bdcp-node -- --config "$BOB_CFG" \
    > "$BOB_LOG" 2>&1 &
echo $! >> "$PID_FILE"
echo "  Bob PID: $!"

# ── Wait for IPC sockets ─────────────────────────────────────
echo ""
echo "=== Waiting for daemons to be ready ==="
for sock in "$ALICE_SOCK" "$BOB_SOCK"; do
    for i in $(seq 1 30); do
        if [[ -S "$sock" ]]; then
            echo "  $sock ready"
            break
        fi
        sleep 1
    done
    if [[ ! -S "$sock" ]]; then
        echo "  FAIL: $sock never appeared"
        echo "--- Relay log ---"
        tail -10 "$RELAY_LOG"
        echo "--- Alice log ---"
        tail -10 "$ALICE_LOG"
        exit 1
    fi
done

# ── Submit via IPC (parallel — relay needs both connections) ─
ALICE_RESULT=$(mktemp)
BOB_RESULT=$(mktemp)

echo ""
echo "=== Submitting Alice values: $RELAY_VALUES (background) ==="
echo "submit $RELAY_VALUES" | nc -U "$ALICE_SOCK" -w 60 > "$ALICE_RESULT" 2>&1 &
ALICE_PID=$!

echo "=== Submitting Bob values: $BOB_VALUES (background) ==="
echo "submit $BOB_VALUES" | nc -U "$BOB_SOCK" -w 60 > "$BOB_RESULT" 2>&1 &
BOB_PID=$!

echo "  Waiting for both to complete..."
wait "$ALICE_PID" "$BOB_PID" || true
echo ""

ALICE_OUT=$(cat "$ALICE_RESULT")
BOB_OUT=$(cat "$BOB_RESULT")
rm -f "$ALICE_RESULT" "$BOB_RESULT"

echo "  Alice response: $ALICE_OUT"
echo "  Bob response:   $BOB_OUT"

# ── Validate ─────────────────────────────────────────────────
echo ""
echo "═══════════════════════════════════════════════════════"
echo "  Validation"
echo "═══════════════════════════════════════════════════════"

if echo "$ALICE_OUT" | grep -q "OK results: \[$EXPECTED\]"; then
    echo "  ✅ Result MATCHES expected [$EXPECTED]"
    echo ""
    echo "  🎉 E2E TEST PASSED"
    exit 0
else
    echo "  ❌ MISMATCH"
    echo "     Expected: [$EXPECTED]"
    echo "     Got:      $ALICE_OUT"
    echo ""
    echo "--- Relay log ---"
    tail -20 "$RELAY_LOG"
    echo "--- Alice log ---"
    tail -10 "$ALICE_LOG"
    echo "--- Bob log ---"
    tail -10 "$BOB_LOG"
    exit 1
fi
