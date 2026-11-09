# HLI — Diseño del scroll y del Transcript Mode

**Commit:** posterior a `e722ec2` · **Estado:** implementado y probado; pendiente
de validación visual en Windows Terminal.

---

## 1. Modelo

`crates/hli-tui/src/viewport.rs`:

```
TranscriptViewport {
    first_visible_line,
    total_rendered_lines,
    viewport_height,
    follow_mode,
    unseen_activity_count,
    wheel_speed,
}
```

Invariantes:

```
0 <= first_visible_line <= max_scroll
max_scroll = max(total_rendered_lines - viewport_height, 0)
```

El transcript se convierte primero en líneas renderizadas
(`wrapped_lines(texto, ancho)`), incluyendo mensajes multilínea y actividad
expandida. Sólo después se calcula la ventana. Cabecera, compositor y barra de
estado quedan fuera del área desplazable.

## 2. Eventos

| Entrada | Acción |
|---|---|
| `MouseEventKind::ScrollUp` | `wheel_speed` líneas arriba |
| `MouseEventKind::ScrollDown` | `wheel_speed` líneas abajo |
| `PgUp` / `PgDn` | media pantalla |
| `Ctrl+Home` / `Ctrl+End` | inicio / final con follow |
| `Home` / `End` sin Ctrl | edición del compositor (sin cambios) |

`wheel_speed` se lee de `HLI_WHEEL_SCROLL_SPEED` (1..=20, por defecto 3). No se
creó un sistema de configuración nuevo.

## 3. Follow

- Al final: `follow_mode = true`; el contenido nuevo baja la ventana.
- Al subir: `follow_mode = false`; `first_visible_line` no cambia y
  `unseen_activity_count` crece.
- Volver al final (`Ctrl+End`, rueda hasta abajo) reinicia el contador.
- Los overlays de Plan y Permission se dibujan sobre el viewport sin mover el
  transcript.

## 4. Scrollbar

Se dibuja sólo si `total_rendered_lines > viewport_height`, con
`ScrollbarState::new(max_scroll()).position(first_visible_line)`. No hay click ni
drag: no se declara interacción de scrollbar.

## 5. Transcript Mode (`Ctrl+O`)

- Vista a pantalla completa con `TuiMode::Transcript`.
- `j/k`, flechas, `Ctrl+U/D`, `Ctrl+B/F`, `PgUp/PgDn`, `g/G`.
- `{`/`}` saltan al prompt anterior o siguiente.
- `/` abre búsqueda; `Enter` la aplica; `Esc` restaura posición y términos.
- `n`/`N` recorren coincidencias; se resaltan con `REVERSED` (no sólo color).
- `Esc`, `q` o `Ctrl+O` vuelven a Agent Mode conservando el compositor.
- El Inspector se abre con `Ctrl+I` y sigue siendo el panel de administración.

## 6. Terminal

`MouseGuard` (RAII) activa `EnableMouseCapture` al entrar y lo desactiva en
`Drop`. El hook de pánico llama a `release_mouse()` antes de `ratatui::restore()`.
`HLI_DISABLE_MOUSE=1` no captura el ratón: Windows Terminal recupera su selección
nativa y el scroll por teclado sigue funcionando.

## 7. Límites de este bloque

- Sin selección interna ni copia con el ratón capturado.
- Sin click/drag en la scrollbar.
- Sin volcado al scrollback nativo (`[` de Claude Code) ni `/transcript export`.
- Un solo renderer: no existe `--no-fullscreen`.
- La altura del área desplazable se aproxima a `alto - 9` (cabecera, compositor y
  estado); el redondeo puede diferir en una línea con layouts reducidos.
