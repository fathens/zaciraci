FROM rust:1.77.2-alpine as builder

RUN apk --no-cache add musl-dev

WORKDIR /app

COPY Cargo.toml .
COPY Cargo.lock .
RUN mkdir src && echo "fn main() {}" > src/main.rs
RUN cargo build --release

COPY src src
RUN touch src/main.rs
RUN cargo build --release
RUN strip target/release/zaciraci -o main

FROM gcr.io/distroless/static-debian12:nonroot
USER nonroot

WORKDIR /app

COPY --from=builder /app/main /app/main

ENTRYPOINT [ "/app/main" ]
