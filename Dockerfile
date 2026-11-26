FROM rust:1.89-bookworm AS builder

WORKDIR /app
COPY . .
RUN cargo build --release

# Agent Skills served over MCP (see src/tools/support/skills.rs), pinned to a
# longbridge/skills commit so an image always serves the skills it was built and
# reviewed with. Bump SKILLS_REF to ship newer skills.
FROM rust:1.89-bookworm AS skills
ARG SKILLS_REPO=https://github.com/longbridge/skills.git
ARG SKILLS_REF=fca9bfa7999c119cd395876ce67d86c442710f8a
WORKDIR /src
RUN git init -q \
    && git remote add origin "$SKILLS_REPO" \
    && git fetch -q --depth 1 origin "$SKILLS_REF" \
    && git checkout -q FETCH_HEAD \
    && test -f skills/longbridge/SKILL.md

FROM debian:bookworm-slim
LABEL io.modelcontextprotocol.server.name="com.longbridge/mcp"

RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/longbridge-mcp /usr/local/bin/longbridge-mcp
COPY --from=skills /src/skills /opt/longbridge/skills
ENV LONGBRIDGE_MCP_SKILLS_DIR=/opt/longbridge/skills

EXPOSE 8000

ENTRYPOINT ["longbridge-mcp", "--bind", "0.0.0.0:8000"]
