# HLI — Guía MCP

`hli-mcp` implementa JSON-RPC 2.0 sobre stdio con framing `Content-Length`, sin
estado de protocolo.

```json
{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","clientInfo":{"name":"cliente","version":"1"}}}
```

Herramientas: `workspace_list`, `thread_read`, `memory_search`,
`memory_commit`, `run_start`, `run_get`, `artifact_read`, `module_list`,
`mch_list`, `mch_inspect`, `mch_invoke`.

Configuración típica del cliente:

```
command: %LOCALAPPDATA%\HLI\bin\hli-mcp.exe
env: HLI_HOME=%LOCALAPPDATA%\HLI\data
```

Limitaciones: no es un servidor MCP universal; cubre las tools listadas y el
baseline 2025-11-25. No hay transporte remoto ni autenticación de red.
