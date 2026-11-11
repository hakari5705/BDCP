# ── Stage 1: builder ──────────────────────────────────────────
FROM rust:slim-bookworm AS builder

WORKDIR /src

# System deps: C toolchain, cmake, go (for aws-lc-sys), perl, pkg-config
RUN apt-get update && apt-get install -y --no-install-recommends \
    cmake golang-go perl pkg-config && \
    rm -rf /var/lib/apt/lists/*

# Copy manifests first for dependency caching
COPY Cargo.toml Cargo.lock ./
COPY bdcp-core/Cargo.toml bdcp-core/
COPY bdcp-net/Cargo.toml bdcp-net/
COPY bdcp-node/Cargo.toml bdcp-node/

# Create dummy lib.rs files so `cargo build` can resolve deps
RUN mkdir -p bdcp-core/src bdcp-net/src bdcp-node/src && \
    echo "fn main() {}" > bdcp-node/src/main.rs && \
    for d in bdcp-core bdcp-net; do \
        printf 'fn main() {}\n' > "$d/src/lib.rs"; \
    done && \
    cargo build --release -p bdcp-node --features tfhe-backend 2>/dev/null || true

# Now copy real source and rebuild
COPY . .

# Target CPU native ensures TFHE gets AVX2/BMI2 instructions
ENV RUSTFLAGS="-C target-cpu=native"
RUN cargo build --release -p bdcp-node --features tfhe-backend

# ── Stage 2: runtime ──────────────────────────────────────────
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y --no-install-recommends \
    socat ca-certificates && \
    rm -rf /var/lib/apt/lists/*

COPY --from=builder /src/target/release/bdcp-node /usr/local/bin/bdcp-node

RUN mkdir -p /etc/bdcp /run/bdcp

WORKDIR /run/bdcp
ENTRYPOINT ["bdcp-node"]
