#!/bin/sh
# Glama uses mcp-proxy which expects a stdio MCP process.
# Our binary now supports --stdio mode directly.
exec longbridge-mcp --stdio
