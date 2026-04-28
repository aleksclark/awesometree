FROM rust:1-bookworm AS builder

WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY core/ core/
COPY src/ src/

RUN cargo build --release --no-default-features --features headless --bin arp-test-server

FROM debian:bookworm-slim

RUN apt-get update && \
    apt-get install -y --no-install-recommends ca-certificates libssl3 && \
    rm -rf /var/lib/apt/lists/*

COPY --from=builder /build/target/release/arp-test-server /usr/local/bin/arp-test-server

ENV ARP_PORT=9099
EXPOSE 9099

ENTRYPOINT ["arp-test-server"]
