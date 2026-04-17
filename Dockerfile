FROM rust:1.89-bookworm AS builder

WORKDIR /app
COPY . .
RUN cargo build --release

FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/longbridge-mcp /usr/local/bin/longbridge-mcp

EXPOSE 8000

ENTRYPOINT ["longbridge-mcp", "--bind", "0.0.0.0:8000"]
