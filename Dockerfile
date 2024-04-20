FROM rust:1.77.2-bookworm as builder

RUN apt update && apt install -y clang

WORKDIR /app

COPY Cargo.toml .
COPY Cargo.lock .
RUN mkdir src && echo "fn main() {}" > src/main.rs
RUN cargo build --release

COPY src src
RUN touch src/main.rs
RUN cargo build --release
RUN strip target/release/zaciraci -o main

FROM debian:bookworm-slim
WORKDIR /app

RUN useradd -ms /bin/bash app
RUN chown -R app /app
USER app

COPY --from=builder /app/main /app/main

ENTRYPOINT [ "/app/main" ]
