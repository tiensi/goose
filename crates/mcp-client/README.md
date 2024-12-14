## Testing stdio

```bash
cargo run -p mcp_client -- --mode git
cargo run -p mcp_client -- --mode echo

cargo run -p mcp_client --bin stdio
```

## Testing SSE

1. Start the MCP server: `fastmcp run -t sse echo.py`
2. Run the client: `cargo run -p mcp_client --bin sse`
