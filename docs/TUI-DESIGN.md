# HLI — Diseño de la TUI (0.5)

**Fecha:** 2026-11-07 · **Crate:** `crates/hli-tui` · **Estado:** implementada y probada en buffer.

## 1. Identidad

| Rol | Color | Uso |
|---|---|---|
| Fondo | negro azulado | terminal, no se pinta explícitamente |
| Texto | gris claro `#C9D1D9` | cuerpo |
| Primario | azul eléctrico `#2F81F7` | cabecera, foco, títulos |
| Éxito | verde `#3FB950` | run completado |
| Advertencia | ámbar `#D29922` | esperando aprobación |
| Error | rojo `#F85149` | run fallido |
| MCH | violeta `#A371F7` | perfiles e instancias |
| Memoria | cian `#39C5CF` | memorias |
| Artefactos | gris azulado `#8B949E` | metadatos |

Accesibilidad:

- `NO_COLOR` desactiva todos los colores.
- Modo monocromático: solo `BOLD`, sin color.
- Los estados se muestran con **texto** además de color (`completed`, `superseded`, `waiting_approval`).
- Sin animaciones; el refresco no mueve el contenido.
- Navegación completa por teclado.

## 2. Layout

Con `width >= 100`:

```
┌ HLI · workspace · branch · provider · daemon ┐
│ Workspaces │ Run        │ pestaña activa     │
│ Threads    │ estado     │                    │
│ Branches   │ tokens     │                    │
├──────────────────────────────────────────────┤
│ > comando                                    │
└──────────────────────────────────────────────┘
```

Con `width < 100`: una sola columna; la fila de pestañas sustituye a los paneles simultáneos.

## 3. Vistas

| Pestaña | Contenido |
|---|---|
| Run | id, estado, provider/model, revisión, tokens, coste, latencia, manifiesto |
| Events | secuencia, tipo, actor, timestamps (últimos 20, orden inverso) |
| Context | presupuesto, estimado, `user_input`, memorias, slices y razón, excluidos, `payload_hash` |
| Memory | agrupada por `active`, `superseded`, `revoked`, `proposed`; autoridad y `superseded_at` |
| Artifacts | id, MIME, bytes, SHA-256; sin previsualización binaria |
| MCH | Profile, Instance, Process y Endpoint separados |

Árbol de ramas: `●` activo, `├─`/`└─` para hermanas, con nombre y `head_sequence`. Los símbolos van
acompañados de texto; no se usan como único indicador.

## 4. Command bar

La barra inferior no implementa un parser propio: divide la línea respetando comillas y ejecuta el
**binario `hli`** con esos argumentos, heredando `HLI_HOME`. El parser real es el de `hli-cli`.
`workspace open <id>` se traduce a `workspace get <id>`.

## 5. Actualización

- Runs activos: 500 ms.
- Eventos: 1 s.
- Listas estables: 4 s.

Sin subscripciones MCP ni eventos remotos. Es polling; el streaming queda para otra fase.

## 6. Preferencias

Diseñadas en TOML:

```toml
[ui]
theme = "dark"
color = "auto"
density = "compact"
motion = "reduced"
default_tab = "run"
```

**No implementadas todavía.** La TUI usa `NO_COLOR` y valores por defecto; el archivo de
preferencias queda pendiente y no se guarda en el workspace ni en el Event Store.

## 7. Dependencias

`ratatui 0.29` + `crossterm 0.28`. **No se añadió `tokio`**: el cliente es TCP bloqueante y el
polling es síncrono. Tokio se incorporará si se migra a streaming; añadirlo ahora sería una
dependencia sin uso.

## 8. Pruebas

`crates/hli-tui/tests/render.rs`, con `TestBackend`:

- `tui_small_terminal_does_not_panic` (40×12)
- `tui_no_color_mode`
- `tui_renders_run_state`
- `tui_renders_superseded_memory`
- `tui_distinguishes_mch_entities`
- `tui_uses_daemon_api_only` (Cargo.toml sin `rusqlite` ni `hli-store`)
- `command_bar_reuses_cli_parser`
- `json_output_remains_unchanged`

## 9. Límites

- Sin edición de memoria desde la TUI; solo inspección.
- Sin filtros interactivos en Events (la vista muestra los últimos 20).
- Sin preferencias persistidas.
- Sin streaming; el refresco puede perder eventos entre polls.
- La command bar ejecuta un subproceso por comando; no hay sesión interactiva.
