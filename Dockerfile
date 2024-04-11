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

FROM gcr.io/distroless/static-debian12:nonroot
USER nonroot

COPY --from=builder /app/target/release/zaciraci /main
WORKDIR /

ENTRYPOINT [ "/main" ]
