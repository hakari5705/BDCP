#!/usr/bin/env bash
# ── BDCP Docker Sanity Check ──────────────────────────────────
# Verifies the deployment is healthy: containers up, IPC sockets
# reachable, computation completes end-to-end.
# Usage:  ./deploy/run.sh [--toy|--tfhe|--zkp]
set -euo pipefail

MODE="${1:---toy}"
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"

echo "═══ BDCP Sanity Check ═══"
echo "Mode: $MODE"
echo ""

# 1. Select configs
case "$MODE" in
    --toy)
        ALICE_CFG="alice.docker.toy.toml"
        BOB_CFG="bob.docker.toy.toml"
        ALICE_VALS="100"
        BOB_VALS="200"
        EXPECTED="300"
        ;;
    --tfhe)
        ALICE_CFG="alice.docker.toml"
        BOB_CFG="bob.docker.toml"
        ALICE_VALS="10,20,30"
        BOB_VALS="1,2,3"
        EXPECTED="11, 22, 33"
        ;;
    --zkp)
        ALICE_CFG="alice.docker.toml"
        BOB_CFG="bob.docker.toml"
        ALICE_VALS="10,20,30"
        BOB_VALS="1,2,3"
        EXPECTED="11, 22, 33"
        ;;
    *) echo "Usage: $0 [--toy|--tfhe|--zkp]"; exit 1 ;;
esac

# 2. Start services with correct configs
cd "$SCRIPT_DIR"
ZKP_FLAG="false"
[[ "$MODE" == "--zkp" ]] && ZKP_FLAG="true"

# Patch the relay config to match the mode
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
else
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
fi

# 3. Launch
echo "→ Starting containers..."
docker compose up --build -d

# 5. Wait for sockets
echo "→ Waiting for IPC sockets..."
SOCK_DIR=$(docker volume inspect bdcp-sockets --format '{{.Mountpoint}}' 2>/dev/null || echo "/var/lib/docker/volumes/bdcp-sockets/_data")

for i in $(seq 1 60); do
    if docker compose exec alice test -S /run/bdcp/alice.sock 2>/dev/null && \
       docker compose exec bob test -S /run/bdcp/bob.sock 2>/dev/null; then
        echo "  ✓ Sockets ready after ${i}s"
        break
    fi
    if [[ $i -eq 60 ]]; then
        echo "  ✗ Timed out waiting for sockets"
        docker compose logs --tail=20
        docker compose down
        exit 1
    fi
    sleep 1
done

# 6. Submit values
echo "→ Submitting Alice values: $ALICE_VALS"
echo "submit $ALICE_VALS" | docker compose exec -T alice socat -t 120 - UNIX-CONNECT:/run/bdcp/alice.sock &
ALICE_PID=$!

sleep 0.5

echo "→ Submitting Bob values: $BOB_VALS"
echo "submit $BOB_VALS" | docker compose exec -T bob socat -t 120 - UNIX-CONNECT:/run/bdcp/bob.sock &
BOB_PID=$!

wait $ALICE_PID 2>/dev/null
wait $BOB_PID 2>/dev/null

# 7. Check results
echo "→ Result:"
ALICE_OUT=$(echo "submit $ALICE_VALS" | timeout 30 socat -t 120 - UNIX-CONNECT:"$SOCK_DIR/alice.sock" 2>/dev/null || echo "TIMEOUT")

if echo "$ALICE_OUT" | grep -q "OK results:"; then
    RESULT=$(echo "$ALICE_OUT" | grep "OK results:" | sed 's/.*OK results: //')
    echo "  ✓ Alice received: $RESULT"
    if [[ "$RESULT" == "[$EXPECTED]" ]]; then
        echo "  ✓ Result matches expected: [$EXPECTED]"
        echo ""
        echo "═══ SANITY CHECK PASSED ═══"
        docker compose down
        exit 0
    else
        echo "  ✗ Result mismatch: expected [$EXPECTED], got $RESULT"
        docker compose down
        exit 1
    fi
else
    echo "  ✗ No OK results in output:"
    echo "$ALICE_OUT"
    docker compose logs --tail=20
    docker compose down
    exit 1
fi
