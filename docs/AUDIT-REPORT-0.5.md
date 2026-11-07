# HLI — Informe de fase 0.5 (interacción)

**Fase:** HLI 0.5 — interfaz operativa CLI/TUI
**Estado:** interacción implementada y probada en buffer; gate visual manual no ejecutado
**Audiencia:** Copilot (auditor externo)

## 1. Qué se completó en este bloque

- Modelo de **foco único** con cinco áreas y ciclo `Tab`/`Shift+Tab`.
- Indicador de foco **no dependiente del color**: borde doble, `▸` y `BOLD`.
- Layout en cinco modos: Full (≥120), Medium (90–119), Compact (60–89), Reduced (<60 o <16) y
  TooSmall (<40 o <10) con mensaje de tamaño.
- Cabecera segmentada `HLI │ workspace / thread / branch │ provider/model │ daemon`.
- Paneles sin duplicación: Navigation, Collection (`Runs`), Inspector (`Run ...`).
- Tabs siempre visibles con `[activa]`; en compacto `3/6 Context`.
- Hint bar contextual por foco.
- Command bar con historial, `Esc`, éxito/error y sugerencia; reutiliza el binario `hli`.
- `/texto` filtra Events por substring de tipo.
- Scroll y selección con indicador `n/total`; selección de run preservada por id tras polling.
- Ayuda superpuesta con `?`, cierre con `Esc`/`?`/`q`.
- ASCII fallback (`*`, `OK`, `RUN`, `X`, `|-`, `` `- ``) y `NO_COLOR`/`TERM=dumb`.
- Estados vacíos accionables con comandos reales.

## 2. Pruebas

`cargo test -p hli-tui`: **23/23**.

Las 15 pruebas de interacción pedidas:

| Prueba | Estado |
|---|---|
| `exactly_one_focus` | ok |
| `tab_cycles_focus` | ok |
| `shift_tab_reverses_focus` | ok |
| `focused_panel_has_noncolor_indicator` | ok |
| `empty_workspace_has_actionable_command` | ok |
| `duplicate_run_titles_are_removed` | ok |
| `tabs_are_visible` | ok |
| `small_layout_uses_single_panel` | ok |
| `tiny_terminal_shows_size_warning` | ok |
| `selection_survives_poll_refresh` | ok |
| `list_scrolls_without_panic` | ok |
| `help_overlay_opens_and_closes` | ok |
| `command_bar_escape_cancels` | ok |
| `command_history_works` | ok |
| `ascii_fallback_contains_no_wide_symbols` | ok |

Más las 8 anteriores de render y contrato.

## 3. Gate visual

| Paso | Estado |
|---|---|
| abrir HLI vacío | implementado; no ejecutado en terminal real |
| crear workspace desde command bar | implementado; probado por parser y subproceso |
| seleccionar workspace | renderizado; sin prueba de interacción real |
| crear thread y branch | implementado vía command bar |
| iniciar run | implementado vía command bar |
| navegar seis tabs | cubierto por tests de tabs |
| inspeccionar memoria y artefactos | cubierto por tests de memoria y MCH |
| abrir y cerrar ayuda | cubierto |
| reducir a <100 columnas | cubierto por layout Compact/Medium |
| probar NO_COLOR | cubierto |
| salir y confirmar daemon up | diseñado; no ejecutado |

**No se generaron capturas** porque el entorno de herramientas no ofrece una terminal interactiva
estable. No se declara el gate visual como superado.

## 4. Riesgos y deuda

- Sin streaming; el polling puede perder eventos entre intervalos.
- El command bar ejecuta un subproceso por comando; no hay sesión interactiva.
- `Alt+1..6` documentado, no implementado.
- El cursor del command bar solo añade/borra al final.
- Preferencias TOML no cargadas.
- La vista de Events filtra por substring de tipo, no por expresión.
- `q` sale solo cuando el command bar no tiene foco.

## 5. Contrato con el Core

- La TUI no abre SQLite y no depende de `hli-store` (test).
- Todos los datos vienen de la API local del daemon.
- CLI y MCP no cambiaron de comportamiento; `--output jsonl` es aditivo.
- No se añadieron endpoints nuevos.

## 6. Verificación

```
cargo build --workspace        → ok
cargo test --workspace         → 23 pruebas TUI + resto en verde
python scripts/mcp_gate.py     → {"gate":"0.3","ok":true}
python scripts/module_gate.py  → {"gate":"0.4","ok":true}
```

## 7. Veredicto

La experiencia de navegación está implementada y cubierta por pruebas de buffer: foco único,
layouts, estados vacíos, tabs, hint, command bar, scroll, ayuda y ASCII. La demostración visual
manual queda pendiente de una terminal real; el informe no la da por hecha.
