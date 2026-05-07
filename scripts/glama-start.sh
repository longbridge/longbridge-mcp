#!/bin/sh
# Start the Longbridge MCP HTTP server in background
longbridge-mcp --bind 0.0.0.0:8000 --base-url http://localhost:8000 &

# Wait up to 10 s for the health endpoint to respond
for i in $(seq 1 10); do
    if curl -sf http://localhost:8000/health > /dev/null 2>&1; then
        break
    fi
    sleep 1
done

# Replace this shell with mcp-proxy pointing at our HTTP server.
# The outer mcp-proxy (invoked with --) will talk to this stdio process,
# which forwards to the HTTP server.
exec mcp-proxy http://localhost:8000/mcp
