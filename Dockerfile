FROM rust:bookworm AS builder

WORKDIR /app
COPY ./Cargo.toml ./Cargo.lock ./
COPY ./src ./src
RUN cargo build --release

FROM gcr.io/distroless/cc
COPY --from=builder /app/target/release/power-usage /power-usage

EXPOSE 9118
USER nonroot
ENTRYPOINT ["/power-usage"]
