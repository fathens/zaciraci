FROM rust:1.85.0-bookworm AS builder
ARG CARGO_BUILD_ARGS

RUN apt update && apt install -y clang

WORKDIR /app

COPY Cargo.toml .
COPY Cargo.lock .
COPY common common
COPY backend backend
COPY frontend frontend

RUN cargo build ${CARGO_BUILD_ARGS} -p zaciraci-backend
RUN cp target/*/zaciraci-backend main

FROM debian:bookworm-slim
WORKDIR /app

RUN apt update && apt install -y openssl ca-certificates libpq5

RUN useradd -ms /bin/bash app
RUN chown -R app /app
USER app

COPY --from=builder /app/main /app/main

ENTRYPOINT [ "/app/main" ]
