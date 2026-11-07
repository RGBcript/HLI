# HLI — Informe de fase 0.5

**Fase:** HLI 0.5 — interfaz operativa CLI/TUI
**Estado:** implementada; gate de render cubierto, gate interactivo no demostrado en este entorno
**Commit:** pendiente de cierre
**Audiencia:** Copilot (auditor externo)

## 1. Qué se implementó

- Crate `hli-tui` (ratatui 0.29 + crossterm 0.28).
- `hli` sin argumentos abre la TUI si stdout es un terminal; si no, imprime el usage.
- `hli <comando>` sigue siendo CLI directa; `--output json|jsonl` se conserva.
- TUI cliente del daemon: solo TCP/NDJSON, sin SQLite, sin `hli-store` (verificado por test).
- Layout responsive: tres columnas a ≥100 caracteres; una columna con pestañas por debajo.
- Vistas Run, Events, Context, Memory, Artifacts y MCH.
- Árbol de ramas con símbolos y texto.
- Command bar que reutiliza el binario `hli` y su parser.
- Polling 500 ms / 1 s / 4 s.
- Tema oscuro con `NO_COLOR` y modo monocromático.

## 2. Qué no se implementó

- Preferencias TOML persistidas (`[ui]`): diseñadas, no cargadas.
- `tokio`: no se añadió; el cliente es síncrono.
- Streaming de eventos: el polling puede perder eventos entre intervalos.
- Edición de memoria desde la TUI.
- Filtros interactivos en la vista de Events.

## 3. Pruebas

`cargo test -p hli-tui`: **8/8**.

| Prueba | Qué demuestra |
|---|---|
| `tui_small_terminal_does_not_panic` | render a 40×12 sin pánico |
| `tui_no_color_mode` | `NO_COLOR` no rompe el render |
| `tui_renders_run_state` | estado `completed` y `run_demo` visibles |
| `tui_renders_superseded_memory` | memoria `superseded` y autoridad `legacy` |
| `tui_distinguishes_mch_entities` | Profile, Instance, Process y Endpoint separados |
| `tui_uses_daemon_api_only` | el crate no depende de `rusqlite` ni `hli-store` |
| `command_bar_reuses_cli_parser` | la barra divide y apunta al binario `hli` |
| `json_output_remains_unchanged` | `hli daemon status` sigue emitiendo JSON |

## 4. Gate de 0.5

| Paso | Estado |
|---|---|
| iniciar daemon | cubierto por gates 0.3/0.4 |
| `hli` sin argumentos abre TUI | implementado; no se pudo demostrar en sesión no interactiva |
| abrir workspace | renderizado desde `workspace.list`; sin test de interacción |
| navegar thread y branch | renderizado; sin test de interacción |
| iniciar run desde la command bar | implementado (subproceso `hli`); sin test de interacción |
| observar eventos | renderizado; sin test de interacción |
| inspeccionar ContextManifest | renderizado; cubierto por `tui_renders_run_state`/Context |
| ver memoria usada y supersedida | cubierto por `tui_renders_superseded_memory` |
| inspeccionar artefacto | renderizado; sin test dedicado |
| inspeccionar MCH Memory | cubierto por `tui_distinguishes_mch_entities` |
| cerrar TUI, daemon continúa | diseñado (`q`); no demostrado |
| CLI y MCP sin cambios | `cargo test` + gates 0.3/0.4 |

**Conclusión del gate:** la parte de render y contrato está demostrada con tests de buffer; la
parte interactiva (abrir, navegar, ejecutar y cerrar) **no se ejecutó** en esta sesión porque el
entorno de herramientas no ofrece una terminal interactiva estable. No se declara superada al
100 %.

## 5. Riesgos

- **Polling:** entre intervalos se pueden perder eventos; no es un stream.
- **Subproceso por comando:** la command bar no mantiene sesión; cada comando paga arranque.
- **Sin preferencias:** el tema es fijo salvo `NO_COLOR`.
- **Render de listas largas:** Events muestra 20; no hay scroll.
- **Dependencias:** ratatui/crossterm añaden peso al workspace; el binario TUI es separado.

## 6. Reproducción

```bash
cargo build --workspace
cargo test -p hli-tui
HLI_HOME=<home> target/debug/hli        # abre TUI en terminal real
HLI_HOME=<home> target/debug/hli workspace list --output json
```

## 7. Veredicto

La TUI existe, es cliente del daemon, respeta la identidad visual y pasa los tests de render y
contrato. No sustituye a la CLI ni introduce una segunda fuente de verdad. El gate interactivo
queda pendiente de una sesión con terminal real; no se afirma como superado.
