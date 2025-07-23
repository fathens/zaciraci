FROM rust:1.85.0-bookworm AS base

RUN cargo install sccache
RUN cargo install cargo-chef

ENV RUSTC_WRAPPER=sccache SCCACHE_DIR=/sccache

FROM base AS planner
WORKDIR /app

COPY Cargo.toml .
COPY Cargo.lock .
COPY common common
COPY backend backend
COPY cli_tokens cli_tokens

RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=$SCCACHE_DIR,sharing=locked \
    cargo chef prepare --recipe-path recipe.json

FROM base AS builder
ARG CARGO_BUILD_ARGS

WORKDIR /app

COPY --from=planner /app/recipe.json recipe.json
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=$SCCACHE_DIR,sharing=locked \
    cargo chef cook --release --recipe-path recipe.json

COPY Cargo.toml .
COPY Cargo.lock .
COPY common common
COPY backend backend
COPY cli_tokens cli_tokens

RUN cargo clean
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=$SCCACHE_DIR,sharing=locked \
    cargo build ${CARGO_BUILD_ARGS} -p zaciraci-backend

RUN cp target/*/zaciraci-backend main

FROM debian:bookworm-slim
WORKDIR /app

RUN apt update && apt install -y openssl ca-certificates libpq5

RUN useradd -ms /bin/bash app
RUN chown -R app /app
USER app

COPY --from=builder /app/main /app/main

ENTRYPOINT [ "/app/main" ]
