# BDCP — Blind Data Collaboration Protocol

BDCP is an experimental Rust project exploring how organizations can
collaborate on data while limiting exposure of their inputs. It provides
homomorphic-encryption backends, a QUIC-based relay, client and daemon
components, and Docker examples.

> **Prototype notice:** This repository is for research and demonstration, not
> production use. The default toy cipher is insecure, and development TLS
> settings are not suitable for deployment with sensitive data. The project
> has not undergone an independent security audit.

## Components

| Path | Description |
|---|---|
| `bdcp-core` | Protocol types and cryptographic abstractions, including toy and TFHE-rs backends |
| `bdcp-net` | QUIC networking and data/control streams |
| `bdcp-cli` | Command-line relay, Alice, and Bob |
| `bdcp-node` | Configurable daemon with IPC support |
| `examples/` | Proof-of-concept and network examples |
| `deploy/` | Docker Compose deployment and configuration |
| `docs/`, `research/` | Design, governance, and research notes |

## Requirements

- Rust 1.91 or newer
- Docker Engine and Docker Compose V2 for the containerized demo
- For TFHE workloads, an x86-64 CPU with AVX2/BMI2 support is recommended

## Run the Docker demo

```sh
cd deploy

# Fast local test with the insecure toy backend
bash run.sh --toy

# TFHE and TFHE with zero-knowledge proofs
bash run.sh --tfhe
bash run.sh --zkp
```

See [`deploy/README.md`](deploy/README.md) for configuration, resource
requirements, and manual operation.

## Build and test

From the repository root:

```sh
cargo test --workspace
```

## License

This project is distributed under the [MIT License](LICENSE).