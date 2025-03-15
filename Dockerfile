FROM rust:1.85.0-bookworm AS builder
ARG CARGO_BUILD_ARGS

RUN apt update && apt install -y clang

WORKDIR /app

COPY Cargo.toml .
COPY Cargo.lock .
RUN mkdir src && echo "fn main() {}" > src/main.rs
RUN cargo build ${CARGO_BUILD_ARGS}

COPY src src
RUN touch src/main.rs
RUN cargo build ${CARGO_BUILD_ARGS}
RUN if [ "x$RUST_BACKTRACE" == "x0"]; then strip target/*/zaciraci -o main; else cp target/*/zaciraci main; fi

FROM debian:bookworm-slim
WORKDIR /app

RUN apt update && apt install -y openssl ca-certificates libpq5

RUN useradd -ms /bin/bash app
RUN chown -R app /app
USER app

COPY --from=builder /app/main /app/main

ENTRYPOINT [ "/app/main" ]
