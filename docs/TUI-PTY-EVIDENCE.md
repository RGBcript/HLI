# HLI — Evidencia automática del gate visual (consola real + PTY)

**Fecha:** 2026-11-08 · **Commit del código:** `7a53f12` · **Versión:** `0.9.0-beta.1`
**Estado:** prevalidación. **No sustituye la firma humana de
`docs/TUI-MANUAL-GATE.md`.**

---

## 1. Método

| Arnés | Uso | Nota |
|---|---|---|
| **Consola real**: `CreateProcess` + `AttachConsole` + `WriteConsoleInputW` + `ReadConsoleOutputCharacterW` | Eventos e interacción | Inyecta eventos `KEY_EVENT` reales; permite `Press`, `Release` y `Repeat` por separado |
| **ConPTY** (pywinpty + pyte) | Tamaños y secuencias de control | Solo render: escribir bytes en el relay sintetiza *down+up*, así que no se usa para validar teclado |

El primer arnés (ConPTY) duplicaba cada tecla **por su mecanismo de relay**, no
por la TUI. Tras corregir K1 esa duplicación ya no tendría efecto, pero toda la
validación de teclado se hizo en consola real, que es el camino fiable.

Entorno limpio (`hli-gate-final`), construido con los comandos reales de
`hli --help`:

- workspace `gate`, thread `session`, rama `main`;
- run `run_d2330d4abf40ec9a` (`completed`, `sim/sim-1`);
- 1 artefacto, 2 memorias (una `active`, una `superseded`);
- módulo `hli-memory` e instancia MCH `memory-main` en estado `ready`;
- daemon `running: true` durante toda la sesión.

---

## 2. Correcciones verificadas

### K1 — eventos `Release` (corregido en `7a53f12`)

`App::handle_key` descarta todo lo que no sea `Press` o `Repeat`:

```rust
if !matches!(k.kind, KeyEventKind::Press | KeyEventKind::Repeat) {
    return false;
}
```

| Caso | Antes | Después |
|---|---|---|
| `Press('a') + Release('a')` | `aa` | `a` |
| `Release('a')` solo | `a` | sin cambio |
| `Press(Tab) + Release(Tab)` | dos paneles | un panel |
| `Press('?') + Release('?')` | abre y cierra | permanece abierta |
| `Press('a','b','c')` + releases | `aabbcc` | `abc` |
| `Repeat` | — | se procesa (una vez por evento) |

### K2 — feedback de la barra (corregido en `7a53f12`)

- `command_input` y `command_feedback` separados.
- `CommandFeedback { kind, message, created_at }`, TTL 5 s.
- Prioridad de render: entrada → feedback vigente → sugerencia.
- El feedback se limpia al empezar a escribir, con `Esc`, al vencer el TTL o al
  llegar uno nuevo.
- El mensaje resume la salida: si es JSON, usa `id`/`name`/`state`; si no, la
  primera línea útil.

| Caso | Antes | Después |
|---|---|---|
| `memory commit` | `suggestion: :workspace create demo` | `✓ ok · mem_d6b133d6ec2213ca` (5 s) |
| `workspace get noexiste` | sugerencia | `× hli: not_found: workspace noexiste no existe` |
| `Esc` tras el comando | — | vuelve a la sugerencia |

---

## 3. Resultados (post-corrección)

| Elemento | Resultado | Evidencia |
|---|---|---|
| Foco inicial | `Navigation`, borde doble, `▸` | `A-eventos.md` §01 |
| `Tab` una vez | avanza **un** panel (`Runs`) | A §02 |
| `?` | abre ayuda y permanece | A §03–04 |
| `abc` | escribe `abc` exacto | A §05 |
| Solo `Release` | ninguna acción | A §06 |
| `Repeat` | una acción por evento | A §07 |
| Ciclo de foco | `Runs` → `Run <id>` → detalle → command bar | `B-recorrido.md` §11.1–11.4 |
| Pestañas `1..6` | las seis activan | B §13.1–13.6 |
| Feedback éxito | `✓ ok · mem_…` | B §15 |
| Feedback error | `× hli: not_found: …` | B §16 |
| `Esc` limpia feedback | vuelve la sugerencia | B §17 |
| Selección | `Down` cambia el run inspeccionado | B §14–14b |
| Scroll | `Down` avanza el contador de eventos | B §18–19 |
| 120×30 | Full | `C-layouts.md` |
| 99×30 | Medium | C |
| 70×20 | Compact (`1/5 Navigation`) | C |
| 59×20 | Reduced (sin `n/5`) | C |
| 39×9 | TooSmall (`Current: 39x9`) | C |
| `NO_COLOR=1` | sin SGR de color; foco visible | `D-no-color.md` |
| `HLI_ASCII=1` | sin `●`; `*` presente | `E-ascii.md` |
| Salida | `exit=0` en todos los escenarios | `resultados.json` |
| Alternate screen | `ESC[?1049h` ×1, `ESC[?1049l` ×1 | `resultados.json` |
| Cursor | `ESC[?25l` ×12 durante el dibujo, `ESC[?25h` ×1; `visible=true` al salir | `resultados.json` |
| Daemon | `running: true` tras la sesión | comprobado con `hli daemon status` |

---

## 4. Pruebas unitarias añadidas

En `crates/hli-tui/tests/render.rs` (15 nuevas, 43 en total en el crate):

- K1: `key_press_is_processed_once`, `key_release_is_ignored`,
  `key_repeat_is_processed`, `press_and_release_do_not_duplicate_input`,
  `question_mark_does_not_open_and_close_help`,
  `tab_advances_exactly_one_focus_area`, `command_text_is_not_duplicated`.
- K2: `command_success_feedback_is_visible`, `command_error_feedback_is_visible`,
  `empty_input_does_not_hide_feedback`, `typing_clears_previous_feedback`,
  `escape_clears_feedback`, `feedback_expires_after_timeout`,
  `new_feedback_replaces_previous_feedback`,
  `command_execution_clears_input_but_keeps_feedback`.

---

## 5. Capturas

| Archivo | Contenido |
|---|---|
| `tui-evidence/A-eventos.md` | Eventos de teclado, antes/después de cada gesto |
| `tui-evidence/B-recorrido.md` | Foco, pestañas, feedback, selección y scroll |
| `tui-evidence/C-layouts.md` | 120×30, 99×30, 70×20, 59×20, 39×9 |
| `tui-evidence/D-no-color.md` | `NO_COLOR=1` |
| `tui-evidence/E-ascii.md` | `HLI_ASCII=1` |
| `tui-evidence/resultados.json` | Resultados, códigos de salida y secuencias |

---

## 6. Límites

Lo que **no** prueba:

- Que el render se vea bien en Windows Terminal (fuente, DPI, tema real).
- Que el teclado físico se comporte exactamente como el sintético; el paso 0 del
  gate lo confirma en el equipo del usuario.
- Tamaños intermedios no muestreados ni comportamiento en Linux.

Lo que **sí** prueba, con binarios reales y datos reales:

- Que K1 y K2 están corregidos y cubiertos por pruebas.
- Que textos, estados y umbrales de layout son los documentados.
- Que la salida es limpia y el daemon sigue vivo.
- Que `NO_COLOR` y `HLI_ASCII` hacen lo que dicen.
