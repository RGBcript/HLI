# AGENTS.md — HLI

Instrucciones para agentes que trabajen en este repositorio. Léelas antes de tocar código.

## Qué es HLI

Runtime agéntico local: event log append-only, planes durables con efectos y permisos por
invocación, workspace → thread → branch, MCP y MCH. El CLI (`hli`) es la interfaz principal.

## Comandos

```bash
cargo build --workspace            # debug
cargo build --release --workspace  # release
cargo test --workspace             # 400+ pruebas
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
```

Gates (deben quedar en `ok=true`):

```bash
python scripts/mcp_gate.py
python scripts/module_gate.py
python scripts/mch_supervisor_gate.py
python scripts/module_signing_gate.py
python scripts/migration_gate.py
python scripts/explicit_reference_gate.py
python scripts/jsonl_follow_gate.py
python scripts/planning_approval_gate.py
python scripts/tool_permission_gate.py
python scripts/agent_scroll_gate.py
python scripts/install_gate.py
powershell -File scripts/agent_tui_gate.ps1   # manual_pending=true es lo esperado
```

## Reglas del proyecto

1. **No dupliques el motor.** La TUI, el CLI y MCP son clientes del daemon; nunca escriben en SQLite.
2. **Un solo layout.** Alturas en `layout::agent_regions`; dibujo y `observe_terminal` lo consumen.
   Nada de `-9`, `+9` ni `transcript_top` sueltos.
3. **Nada de red en el hilo de dibujo.** Usa `daemon_async`; `call_ui` es la red de seguridad.
4. **Sanea el texto** que venga del modelo antes de pintarlo o exportarlo (`sanitize`).
5. **Poda el transcript** (`MAX_LINES`); el Event Store conserva la verdad.
6. **Ancho de pantalla, no caracteres**: `unicode-width` en cualquier conteo de líneas.
7. **Un thread solo vale en su workspace** (`ensure_thread` comprueba `workspace_id`).
8. **`shell.execute` está denegado** por diseño. No añadas bypass.
9. **Aprobar el plan no aprueba la invocación**: cada tool pide su permiso.
10. **No expongas secretos** en receipts, eventos, MCP, UI ni logs. Handles y hashes, nunca valores.

## Estilo

- Rust 2024, `rustfmt` del repo, clippy sin warnings.
- Mensajes de commit en el estilo existente: `feat(tui): …`, `fix(agent): …`, `test(cli): …`.
- Los tests visuales comprueban estado y texto mínimo; no los acoples a cadenas exactas.
- Toda corrección lleva su prueba; si es un bug, la prueba debe fallar antes del arreglo.

## Qué no tocar sin permiso explícito

- Física, parámetros y axiomas de TUEC/PUPC.
- `dist/`, etiquetas y publicación de versiones.
- Política de permisos, `SYSTEM_PROMPT` y contratos de receipts.
- Migraciones del store (subir versión exige gate y guía de migración).
