## Testing stdio

```bash
cargo run -p mcpclient -- --mode git
cargo run -p mcpclient -- --mode echo

cargo run -p mcpclient --bin stdio
```

## Testing SSE

1. Start the MCP server: `fastmcp run -t sse echo.py`
2. Run the client: `cargo run -p mcpclient --bin sse`
