FROM rust:1.89-bookworm AS builder

WORKDIR /app
COPY . .
RUN cargo build --release

FROM debian:bookworm-slim
LABEL io.modelcontextprotocol.server.name="com.longportapp/mcp"

RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/longport-mcp /usr/local/bin/longport-mcp

EXPOSE 8000

ENTRYPOINT ["longport-mcp", "--bind", "0.0.0.0:8000"]
