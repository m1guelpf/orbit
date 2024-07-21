FROM lukemathwalker/cargo-chef:latest-rust-bookworm AS chef
WORKDIR /orbit

FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder
COPY --from=planner /orbit/recipe.json recipe.json
RUN cargo chef cook --release --recipe-path recipe.json
COPY . .
RUN cargo build --release --bin orbit

FROM debian:bookworm-slim AS runtime
WORKDIR /orbit
COPY --from=builder /orbit/target/release/orbit /usr/local/bin

EXPOSE 8000
VOLUME ["/storage"]
ENTRYPOINT ["/usr/local/bin/orbit"]

HEALTHCHECK --interval=5m \
    CMD curl -f http://localhost:8000/ || exit 1
