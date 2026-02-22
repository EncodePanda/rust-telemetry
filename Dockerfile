FROM rust:latest AS builder
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
RUN mkdir src && echo "fn main() {}" > src/main.rs
RUN cargo build --release
RUN rm -rf src
COPY src ./src
COPY migrations ./migrations
RUN touch src/main.rs
RUN cargo build --release

FROM debian:bookworm-slim
WORKDIR /app
COPY --from=builder /app/target/release/rust-telemetry ./rust-telemetry
EXPOSE 3000
ENTRYPOINT ["./rust-telemetry"]
