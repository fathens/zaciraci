FROM rust:1.94.1-bookworm AS base

RUN cargo install sccache cargo-chef

ENV RUSTC_WRAPPER=sccache SCCACHE_DIR=/sccache

# Build diesel_cli in an independent stage (unaffected by source changes)
FROM base AS diesel-builder
RUN apt-get update && apt-get install -y libpq-dev && rm -rf /var/lib/apt/lists/*
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=$SCCACHE_DIR,sharing=locked \
    cargo install diesel_cli --no-default-features --features postgres

FROM base AS planner
WORKDIR /app

COPY Cargo.toml .
COPY Cargo.lock .
COPY crates crates

RUN --mount=type=cache,target=/usr/local/cargo/registry \
    cargo chef prepare --recipe-path recipe.json

FROM base AS builder

RUN apt-get update && apt-get install -y protobuf-compiler && rm -rf /var/lib/apt/lists/*

WORKDIR /app

COPY --from=planner /app/recipe.json recipe.json
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=$SCCACHE_DIR,sharing=locked \
    cargo chef cook --release --recipe-path recipe.json

COPY Cargo.toml .
COPY Cargo.lock .
COPY crates crates

ARG GIT_COMMIT_HASH=unknown
ENV GIT_COMMIT_HASH=$GIT_COMMIT_HASH

RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=$SCCACHE_DIR,sharing=locked \
    cargo build --release -p backend

RUN cp target/release/backend main

FROM debian:bookworm-slim
WORKDIR /app

RUN apt-get update && apt-get install -y openssl ca-certificates libpq5 && rm -rf /var/lib/apt/lists/*

RUN useradd -ms /bin/bash app
RUN chown -R app /app
USER app

COPY --from=builder /app/main /app/main
COPY --from=diesel-builder /usr/local/cargo/bin/diesel /usr/local/bin/diesel
COPY migrations /app/migrations

CMD [ "/app/main" ]
