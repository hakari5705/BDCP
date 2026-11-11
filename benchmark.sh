#!/usr/bin/env bash
set -uo pipefail
shopt -s nullglob

# ═══════════════════════════════════════════════════════════════
# BDCP Performance Benchmark
# Measures end-to-end latency, throughput, and resource usage
# for multiple batch sizes and crypto configurations.
# ═══════════════════════════════════════════════════════════════

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
RESULTS_DIR=$(mktemp -d)
trap 'rm -rf "$RESULTS_DIR"' EXIT

NC_TIMEOUT=120

# ── Scenarios ────────────────────────────────────────────────
declare -a SCENARIOS=()
BASE_PORT=$((19500 + RANDOM % 200))
SCENARIO_IDX=0

add_scenario() {
    SCENARIOS+=("$1:$2:$3:$4")
}

# (label, alice_vals, bob_vals, features)
FEATURES="--features tfhe-backend"
add_scenario "toy-1v"       "100"       "200"       "$FEATURES"
add_scenario "tfhe-1v"      "42"        "58"        "$FHEATURES"
add_scenario "tfhe-10v"     "1,2,3,4,5,6,7,8,9,10"  "10,20,30,40,50,60,70,80,90,100" "$FHEATURES"
add_scenario "tfhe+zkp-3v"  "10,20,30"  "1,2,3"     "$FHEATURES"

run_scenario() {
    local label="$1" alice_vals="$2" bob_vals="$3" features="$4"
    local relay_port=$((19500 + SCENARIO_IDX + (RANDOM % 100)))
    local alice_sock="/tmp/bdcp_bench_${label}_alice.sock"
    local bob_sock="/tmp/bdcp_bench_${label}_bob.sock"

    local backend zkp
    if [[ "$label" == toy-1v ]]; then
        backend="toy"; zkp="false"
    elif [[ "$label" == tfhe+zkp-3v ]]; then
        backend="tfhe"; zkp="true"
    else
        backend="tfhe"; zkp="false"
    fi

    local relay_cfg=$(mktemp) alice_cfg=$(mktemp) bob_cfg=$(mktemp)

    cat > "$relay_cfg" <<EOF
[node]
name = "bench-relay-${label}"
role = "relay"
listen_addr = "0.0.0.0:${relay_port}"

[network]
keep_alive_secs = 10
idle_timeout_secs = 120

[crypto]
backend = "${backend}"
enable_zkp = ${zkp}
EOF

    cat > "$alice_cfg" <<EOF
[node]
name = "bench-alice-${label}"
role = "alice"
listen_addr = "127.0.0.1:0"

[network]
relay_addr = "127.0.0.1:${relay_port}"
keep_alive_secs = 10
idle_timeout_secs = 120

[crypto]
backend = "${backend}"
enable_zkp = ${zkp}

[ipc]
socket_path = "${alice_sock}"
EOF

    cat > "$bob_cfg" <<EOF
[node]
name = "bench-bob-${label}"
role = "bob"
listen_addr = "127.0.0.1:0"

[network]
relay_addr = "127.0.0.1:${relay_port}"
keep_alive_secs = 10
idle_timeout_secs = 120

[crypto]
backend = "${backend}"
enable_zkp = ${zkp}

[ipc]
socket_path = "${bob_sock}"
EOF

    rm -f "$alice_sock" "$bob_sock"

    local BDCP_BIN="./target/release/bdcp-node"
    # shellcheck disable=SC2086
    $BDCP_BIN --config "$relay_cfg" \
        > "$RESULTS_DIR/relay_${label}.log" 2>&1 &
    local relay_pid=$!
    # shellcheck disable=SC2086
    $BDCP_BIN --config "$alice_cfg" \
        > "$RESULTS_DIR/alice_${label}.log" 2>&1 &
    local alice_pid=$!
    # shellcheck disable=SC2086
    $BDCP_BIN --config "$bob_cfg" \
        > "$RESULTS_DIR/bob_${label}.log" 2>&1 &
    local bob_pid=$!

    # Wait for IPC sockets
    for sock in "$alice_sock" "$bob_sock"; do
        for i in $(seq 1 90); do
            [[ -S "$sock" ]] && break
            sleep 1
        done
        if [[ ! -S "$sock" ]]; then
            echo "  ┃ ⚠ $label: $sock not ready in 90s" >&2
            kill "$relay_pid" "$alice_pid" "$bob_pid" 2>/dev/null
            rm -f "$relay_cfg" "$alice_cfg" "$bob_cfg"
            return 1
        fi
    done

    # Submit in parallel (relay needs both connections).
    # socat stays connected after stdin EOF, unlike Ncat which quits immediately.
    local alice_res_file=$(mktemp)
    local bob_res_file=$(mktemp)

    local alice_start alice_end alice_elapsed
    alice_start=$(date +%s%N)
    # -t 120 keeps socket alive 120s after stdin half-close (default 0.5s kills before QUIC completes)
    echo "submit $alice_vals" | timeout $NC_TIMEOUT socat -t 120 - UNIX-CONNECT:"$alice_sock" > "$alice_res_file" 2>&1 &
    local alice_socat_pid=$!

    sleep 0.5

    local bob_start bob_end bob_elapsed
    bob_start=$(date +%s%N)
    echo "submit $bob_vals" | timeout $NC_TIMEOUT socat -t 120 - UNIX-CONNECT:"$bob_sock" > "$bob_res_file" 2>&1 &
    local bob_socat_pid=$!

    wait "$alice_socat_pid" "$bob_socat_pid" 2>/dev/null
    alice_end=$(date +%s%N)
    bob_end=$(date +%s%N)
    alice_elapsed=$(( (alice_end - alice_start) / 1000000 ))
    bob_elapsed=$(( (bob_end - bob_start) / 1000000 ))

    local alice_out=$(cat "$alice_res_file")
    local bob_out=$(cat "$bob_res_file")
    rm -f "$alice_res_file" "$bob_res_file"

    sleep 2

    # Memory (RSS) from /proc at peak (after computation)
    # relay_pid/alice_pid/bob_pid are the direct bdcp-node process PIDs
    local relay_mem="N/A" alice_mem="N/A" bob_mem="N/A"
    [[ -f "/proc/$relay_pid/status" ]] && relay_mem=$(awk '/VmRSS/{print $2}' "/proc/$relay_pid/status" 2>/dev/null)
    [[ -f "/proc/$alice_pid/status" ]] && alice_mem=$(awk '/VmRSS/{print $2}' "/proc/$alice_pid/status" 2>/dev/null)
    [[ -f "/proc/$bob_pid/status" ]] && bob_mem=$(awk '/VmRSS/{print $2}' "/proc/$bob_pid/status" 2>/dev/null)

    kill "$relay_pid" "$alice_pid" "$bob_pid" 2>/dev/null
    wait 2>/dev/null

    rm -f "$relay_cfg" "$alice_cfg" "$bob_cfg"

    # Parse result
    local alice_result="FAIL"
    if echo "$alice_out" | grep -q "OK results:"; then
        alice_result=$(echo "$alice_out" | grep "OK results:" | sed 's/.*OK results: //')
    fi

    # Network transfer sizes from relay log (strip ANSI color codes)
    local rlog="$RESULTS_DIR/relay_${label}.log"
    # Strip ANSI escapes like [32m, [2m, etc.
    local clean=$(sed 's/\x1b\[[0-9;]*m//g' "$rlog")
    local crs_size ek_size alice_ct_size bob_ct_size
    crs_size=$(echo "$clean" | grep -oP 'Received CRS \(\K\d+' | head -1 || echo "-")
    ek_size=$(echo "$clean" | awk '/Received [0-9]+ bytes/ && !/CRS/ && !/Bob/ {count++; if(count==1){print $5; exit}}' 2>/dev/null || echo "-")
    alice_ct_size=$(echo "$clean" | awk '/Received [0-9]+ bytes/ && !/CRS/ && !/Bob/ {count++; if(count==4){print $5; exit}}' 2>/dev/null || echo "-")
    bob_ct_size=$(echo "$clean" | grep -oP "Received Bob's data \(\K\d+" | head -1 || echo "-")

    # Save detailed phase report
    {
        echo "=== Phase Report: $label ==="
        echo "  Alice values: $alice_vals"
        echo "  Bob   values: $bob_vals"
        echo "  End-to-end:   ${alice_elapsed}ms"
        echo ""
        echo "  Network bytes:"
        [[ "$zkp" == "true" ]] && echo "    CRS:           ${crs_size} B"
        echo "    CompServerKey: ${ek_size} B"
        echo "    Alice batch:   ${alice_ct_size} B"
        echo "    Bob data:      ${bob_ct_size} B"
        echo ""
        echo "  Memory (RSS):"
        echo "    Relay:  ${relay_mem} kB"
        echo "    Alice:  ${alice_mem} kB"
        echo "    Bob:    ${bob_mem} kB"
        echo ""
        echo "  Alice stdout: $alice_out"
        echo "  Bob   stdout: $bob_out"
        echo ""
    } >> "$RESULTS_DIR/phase_report.txt"

    # Tab-separated summary line
    printf "%-14s %-16s %-16s %10s %10s %8s %8s %8s  %s\n" \
        "$label" "$alice_vals" "$bob_vals" "${alice_elapsed}ms" "${bob_elapsed}ms" \
        "${relay_mem}" "${alice_mem}" "${bob_mem}" "$alice_result"
}

# ═══════════════════════════════════════════════════════════════
# Main
# ═══════════════════════════════════════════════════════════════
echo "═══════════════════════════════════════════════════════════════════"
echo "  BDCP Performance Benchmark"
echo "  $(date)"
echo "═══════════════════════════════════════════════════════════════════"
echo ""

# Build first (excluding the scenarios from time measurement)
# Build once with all features needed
echo "→ Building bdcp-node (release + tfhe-backend)..."
cargo build --release -p bdcp-node --features tfhe-backend 2>/dev/null
echo ""

# Table header
printf "%-14s %-16s %-16s %10s %10s %8s %8s %8s  %s\n" \
    "Scenario" "Alice In" "Bob In" "AliceTime" "BobTime" "R-Mem" "A-Mem" "B-Mem" "Result"
printf "%s\n" "─────────────────────────────────────────────────────────────────────────────────────────────────"

for scenario in "${SCENARIOS[@]}"; do
    IFS=':' read -r label alice_vals bob_vals features <<< "$scenario"
    run_scenario "$label" "$alice_vals" "$bob_vals" "$features"
    SCENARIO_IDX=$((SCENARIO_IDX + 1))
done

echo ""
echo "═══════════════════════════════════════════════════════════════════"
echo "  Detailed Phase Reports"
echo "═══════════════════════════════════════════════════════════════════"
cat "$RESULTS_DIR/phase_report.txt"
