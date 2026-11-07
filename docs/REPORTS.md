# HLI — Informes por fase

Un informe por fase. Los de 0.1 y 0.2 documentan implementación verificada; los de 0.3 y 0.4
documentan alcance y gate, y declaran explícitamente que **no están iniciadas**.

| Fase | Estado | Commit | Informe |
|---|---|---|---|
| 0.1 — durabilidad | cerrada | `fae01d4` | [AUDIT-REPORT-0.1.md](AUDIT-REPORT-0.1.md) |
| 0.2 — contexto y ejecución | cerrada | `1a484dd` | [AUDIT-REPORT-0.2.md](AUDIT-REPORT-0.2.md) |
| 0.3 — MCP y HLI Memory | cerrada | `HEAD` | [AUDIT-REPORT-0.3.md](AUDIT-REPORT-0.3.md) |
| 0.4 — Module SDK y MCH | cerrada | `HEAD` | [AUDIT-REPORT-0.4.md](AUDIT-REPORT-0.4.md) |

## Cómo leerlos

- **0.1 y 0.2**: cada afirmación remite a un comando reproducible en su sección 2. Si no se
  reproduce, es un defecto del informe.
- **0.3**: evidencia ejecutable en `scripts/mcp_gate.py`; el informe resume el gate.
- **0.4**: evidencia ejecutable en `scripts/module_gate.py`; el informe resume el gate.

## Documentos relacionados

- `docs/CONFORMANCE.md` — matriz viva especificación vs código, actualizada al cerrar cada gate.
- `docs/adr/` — decisiones materiales (0001–0006).
- `README.md` — estado, compilación y comandos.
