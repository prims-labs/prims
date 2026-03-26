FROM rust:1-bookworm AS builder
WORKDIR /app

RUN apt-get update && apt-get install -y --no-install-recommends \
    clang \
    libclang-dev \
    pkg-config \
    cmake \
    build-essential \
    && rm -rf /var/lib/apt/lists/*

COPY Cargo.toml ./
COPY src ./src
COPY benches ./benches

RUN cargo build --release --bin prims

FROM debian:bookworm-slim
WORKDIR /app

COPY --from=builder /app/target/release/prims /usr/local/bin/prims

EXPOSE 7001

CMD ["prims"]
