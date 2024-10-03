FROM rust:1.81.0-bookworm as builder
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
RUN strip target/*/zaciraci -o main

FROM debian:bookworm-slim
WORKDIR /app

RUN apt update && apt install -y openssl ca-certificates libpq5

RUN useradd -ms /bin/bash app
RUN chown -R app /app
USER app

COPY --from=builder /app/main /app/main

ENTRYPOINT [ "/app/main" ]
