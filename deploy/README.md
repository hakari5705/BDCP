# BDCP — Blind Data Collaboration Protocol

## Quick Start (Docker)

### Prerequisites

- Docker Engine 24+ with Compose V2
- 4 GB+ RAM (8 GB recommended for ZKP mode)
- CPU with AVX2/BMI2 support (most x86-64 CPUs from 2014+)

### Launch the cluster

```bash
cd deploy

# Quick sanity test with toy cipher (~3 seconds)
bash run.sh --toy

# Real TFHE FHE (~45 seconds)
bash run.sh --tfhe

# TFHE + Zero-Knowledge Proofs (~90 seconds)
bash run.sh --zkp
```

### Manual operation

```bash
# 1. Start the cluster in the background
docker compose up --build -d

# 2. Submit encrypted values (Alice, in one terminal)
echo "submit 10,20,30" | socat -t 120 - UNIX-CONNECT:$(docker volume inspect bdcp-sockets --format '{{.Mountpoint}}')/alice.sock

# 3. Submit values (Bob, in another terminal)
echo "submit 1,2,3" | socat -t 120 - UNIX-CONNECT:$(docker volume inspect bdcp-sockets --format '{{.Mountpoint}}')/bob.sock

# 4. Read the result from Alice's output (printed to stdout)

# 5. Tear down
docker compose down
```

### Architecture

```
┌──────────┐    QUIC (UDP 9447)    ┌──────────┐
│  Alice   │──────────────────────▶│          │
│  daemon  │                       │  Relay   │
│ (client) │◀──────────────────────│  daemon  │
└──────────┘    ComputeResult      │          │
                                   │ (server) │
┌──────────┐    QUIC (UDP 9447)    │          │
│   Bob    │──────────────────────▶│          │
│  daemon  │   Encrypted values    └──────────┘
│ (client) │
└──────────┘
      │ IPC (Unix sockets)              │ IPC
      ▼                                 ▼
  Host app                          Host app
  (socat/your code)                 (socat/your code)
```

### Performance

| Scenario | End-to-End | Relay RSS | Network (CompServerKey) |
|---|---|---|---|
| Toy (1 value) | ~2.5 s | 8 MB | — |
| TFHE (1 value) | ~8 s | 192 MB | 60 MB |
| TFHE (10 values) | ~11 s | 198 MB | 60 MB |
| TFHE+ZKP (3 values) | ~44 s | 351 MB | 60 MB + 3 MB CRS |

### Configuration

Edit the TOML files before starting:

| File | Purpose |
|---|---|
| `deploy/bdcp.toml` | Relay config (backend, ZKP toggle) |
| `deploy/alice.docker.toml` | Alice daemon (Docker — uses `relay:9447`) |
| `deploy/bob.docker.toml` | Bob daemon (Docker — uses `relay:9447`) |
| `deploy/alice.docker.toy.toml` | Alice daemon with toy cipher (no TFHE) |
| `deploy/bob.docker.toy.toml` | Bob daemon with toy cipher (no TFHE) |

Key options under `[crypto]`:

- `backend = "tfhe"` — real FHE (requires `--features tfhe-backend` build)
- `backend = "toy"` — fast insecure cipher for testing
- `enable_zkp = true` — zero-knowledge proofs (requires `backend = "tfhe"`)
