FROM rust:1.91.0-bookworm AS base

RUN cargo install sccache
RUN cargo install cargo-chef

ENV RUSTC_WRAPPER=sccache SCCACHE_DIR=/sccache

FROM base AS planner
WORKDIR /app

COPY Cargo.toml .
COPY Cargo.lock .
COPY crates crates

RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=$SCCACHE_DIR,sharing=locked \
    cargo chef prepare --recipe-path recipe.json

FROM base AS builder
ARG CARGO_BUILD_ARGS
ARG GIT_COMMIT_HASH=unknown

RUN apt-get update && apt-get install -y protobuf-compiler && rm -rf /var/lib/apt/lists/*

ENV GIT_COMMIT_HASH=$GIT_COMMIT_HASH

WORKDIR /app

COPY --from=planner /app/recipe.json recipe.json
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=$SCCACHE_DIR,sharing=locked \
    cargo chef cook --release --recipe-path recipe.json

COPY Cargo.toml .
COPY Cargo.lock .
COPY crates crates

RUN cargo clean
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=$SCCACHE_DIR,sharing=locked \
    cargo build ${CARGO_BUILD_ARGS} -p backend

RUN cp target/*/backend main

# Build diesel_cli in the builder stage
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=$SCCACHE_DIR,sharing=locked \
    cargo install diesel_cli --no-default-features --features postgres

FROM debian:bookworm-slim
WORKDIR /app

RUN apt update && apt install -y openssl ca-certificates libpq5 && rm -rf /var/lib/apt/lists/*

RUN useradd -ms /bin/bash app
RUN chown -R app /app
USER app

COPY --from=builder /app/main /app/main
COPY --from=builder /usr/local/cargo/bin/diesel /usr/local/bin/diesel
COPY migrations /app/migrations

ENTRYPOINT [ "/app/main" ]
