# HLI — Diseño de la TUI (0.5)

**Crate:** `crates/hli-tui` · **Estado:** interacción completa, gate visual pendiente de terminal real.

## 1. Modelo de foco

Un único foco activo:

```rust
enum FocusArea { Navigation, Collection, Inspector, DetailTabs, CommandBar }
```

- `Tab` / `Shift+Tab` ciclan en orden.
- El foco se representa con **borde doble**, título `▸` y estilo `BOLD`.
- En monocromo sigue viéndose `▸`; el color nunca es el único indicador.

## 2. Layout

| Ancho / alto | Modo | Composición |
|---|---|---|
| ≥ 120 × 24 | `Full` | Navigation 24 % · Collection 30 % · Inspector 46 %; abajo tabs + detalle |
| 90–119 | `Medium` | Navigation 30 % · Inspector+detalle 70 % |
| 60–89 | `Compact` | un panel visible + selector `n/5 Focus` + tabs |
| < 60 o < 16 filas | `Reduced` | un panel + tabs + command bar |
| < 40 o < 10 filas | `TooSmall` | `Terminal too small / Required: 40x10 / Current: WxH / q quit` |

Cabecera: `HLI │ workspace / thread / branch │ provider/model │ daemon: up`, truncada por la
derecha conservando HLI, daemon y estado.

## 3. Paneles

- **Navigation:** workspaces, threads y árbol de ramas (`●`, `├─`, `└─`; ASCII `*`, `|-`, `` `- ``).
- **Collection:** runs de la rama; título `Runs` (sin duplicar `Run`).
- **Inspector:** objeto seleccionado; título dinámico `Run run_...`, `Memory mem_...`,
  `Artifact art_...`, `MCH memory-main`.
- **DetailTabs:** contenido de la pestaña activa.

## 4. Tabs

Siempre visibles: `[Run] Events Context Memory Artifacts MCH`. La activa va entre corchetes.
En compacto: `3/6 Context`. Controles: `←→`, `1..6` (fuera del command bar), `Alt+1..6` previsto.

## 5. Hint bar

Sobre el command bar, cambia con el foco:

| Foco | Texto |
|---|---|
| Navigation | `↑↓ select · → expand · Enter open · Tab panel · ? help` |
| Collection | `↑↓ select · Enter open · Tab panel · : command` |
| Inspector | `↑↓ scroll · ←→ tabs · ? help` |
| DetailTabs | `←→ tabs · 1..6 direct · ? help` |
| CommandBar | `Enter run · Esc cancel · ↑↓ history` |

## 6. Command bar

- `:` enfoca; `Esc` cancela; `Enter` ejecuta.
- Historial local con `↑↓`.
- `↑` recupera el último comando; `↓` avanza o limpia.
- Éxito en texto normal, error en rojo (o `error:` en mono).
- Sugerencia cuando está vacío: `:workspace create demo`.
- **Reutiliza el binario `hli`**: no hay segundo parser.
- `/texto` aplica filtro a la vista de Events sin ejecutar CLI.

## 7. Listas y scroll

- Selección con `▸`; indicador `n/total`.
- Scroll en Events con `↑↓` cuando el foco está en Inspector/DetailTabs.
- La selección de run se preserva por `run_id` tras el polling; si el id desaparece, cae a 0.

## 8. Estados vacíos

| Panel | Título | Acción |
|---|---|---|
| Navigation | `No workspaces` | `:workspace create <name>` |
| Collection | `No runs on this branch` | `:run start "<task>"` |
| Memory | `No memories selected` | `:memory search <query>` |

Las acciones son comandos reales del CLI.

## 9. Ayuda

`?` abre una superposición con navegación, tabs, command bar, estados y símbolos. Se cierra con
`Esc`, `?` o `q`.

## 10. Accesibilidad

- `NO_COLOR` y `TERM=dumb`.
- Modo monocromático con `BOLD` y `▸`.
- ASCII: `●→*`, `✓→OK`, `↻→RUN`, `×→X`, `├─→|-`, `└─→`-`.
- JSON/JSONL del CLI sin ANSI (sin cambios).
- Sin dependencia de ratón.

## 11. Preferencias

Diseñadas en TOML (`[ui] theme/color/density/motion/default_tab`), **no implementadas**. Se leen
variables de entorno (`NO_COLOR`, `HLI_ASCII`, `TERM`). No se guardan en el workspace ni en el
Event Store.

## 12. Pruebas

23 pruebas en `cargo test -p hli-tui`, incluidas las 15 de interacción: foco único, ciclo y
retroceso, indicador sin color, estado vacío accionable, títulos sin duplicar, tabs visibles,
layout compacto, terminal diminuta, selección tras polling, scroll, ayuda, cancelación, historial
y ASCII.

## 13. Límites

- Sin streaming: polling 500 ms / 1 s / 4 s.
- Sin edición de memoria desde la TUI.
- Filtro de Events por substring de tipo, no por expresión.
- La command bar ejecuta un subproceso por comando.
- `Alt+1..6` documentado, no implementado (los dígitos simples cubren el caso).

## Viewport y scroll (0.9)

`TranscriptViewport` trabaja sobre líneas renderizadas (wrapping incluido):
`first_visible_line`, `total_rendered_lines`, `viewport_height`, `follow_mode`,
`unseen_activity_count` y `wheel_speed`. Cabecera, compositor y barra de estado
quedan fuera del área desplazable. La scrollbar se deriva de `max_scroll` y no es
decorativa. Los overlays de Plan y Permission se dibujan sobre el viewport sin
moverlo.
