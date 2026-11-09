# HLI — Evidencia del scroll y Transcript Mode

**Commit evaluado:** posterior a `e508911` · **Versión:** `0.9.0-beta.1`
**Fecha:** 2026-11-09 · **Plataforma:** Windows 11, Windows Terminal (sin probar
rueda física en esta sesión).

Este documento separa lo demostrado de lo pendiente. No presenta la inyección de
`MouseEvent` como equivalente a una rueda física.

---

## 1. Demostrado por pruebas de buffer

`cargo test -p hli-tui --test render`: **194 pruebas**.

| Comprobación | Pruebas |
|---|---|
| Rueda: arriba/abajo, clamping, foco en compositor | `mouse_wheel_up_changes_visible_lines`, `mouse_wheel_down_changes_visible_lines`, `mouse_wheel_clamps_at_start`, `mouse_wheel_clamps_at_end`, `mouse_scroll_works_with_composer_focused` |
| Velocidad inválida | `invalid_scroll_speed_uses_default` (1–20, default 3) |
| Scrollbar | `scrollbar_matches_viewport_position`, `scrollbar_hidden_when_content_fits` |
| Follow y novedades | `scroll_up_disables_follow_mode`, `new_activity_preserves_historical_position`, `new_activity_increments_unseen_count`, `new_activity_indicator_is_visible`, `jump_to_bottom_clears_unseen_count`, `wheel_to_bottom_reenables_follow` |
| Teclado | `page_up_scrolls_half_viewport`, `page_down_scrolls_half_viewport`, `ctrl_home_moves_to_start`, `ctrl_end_reenables_follow` |
| Resize y wrapping | `resize_recalculates_wrapped_lines`, `resize_clamps_scroll_position`, `resize_preserves_follow_mode`, `expanded_activity_changes_total_lines_safely`, `long_response_keeps_composer_visible`, `narrow_terminal_keeps_jump_indicator_readable` |
| Overlays sobre historia | `plan_overlay_is_visible_while_scrolled_up`, `permission_overlay_is_visible_while_scrolled_up`, `overlay_does_not_enable_follow_mode`, `closing_overlay_preserves_scroll_position` |
| Transcript Mode | `ctrl_o_opens_transcript_mode`, `ctrl_i_opens_inspector_not_transcript`, `transcript_scrolls_one_line`, `transcript_scrolls_half_page`, `transcript_scrolls_full_page`, `transcript_jumps_to_start_and_end`, `transcript_jumps_between_user_prompts`, `transcript_search_finds_visible_text`, `transcript_search_finds_tool_activity`, `transcript_search_cancel_restores_position`, `transcript_exit_preserves_composer`, `transcript_exit_preserves_agent_scroll` |
| Sin ratón | `disable_mouse_preserves_keyboard_scroll`, `disable_mouse_preserves_transcript_mode` |
| K5 | `accepted_prompt_clears_real_composer_state`, `failed_submission_preserves_real_composer_state`, `resume_does_not_restore_last_prompt_into_composer` |

`cargo test -p hli-tui --lib`: **12 pruebas** del modelo (`max_scroll`,
clamping, follow, PgUp/PgDn, resize, wrapping, thumb, velocidad).

## 2. Demostrado por eventos de mouse inyectados

Los tests construyen `crossterm::event::MouseEvent` con
`MouseEventKind::ScrollUp` y `ScrollDown` y los pasan por `App::handle_mouse`,
el mismo handler que usa el bucle real. Esto demuestra el cableado y la
aritmética; **no** demuestra que Windows Terminal entregue esos eventos ni que la
rueda física se sienta bien.

## 3. Gate automático

`scripts/agent_scroll_gate.py`: `ok=true` con `mouse_wheel`, `scrollbar`,
`follow_mode`, `new_activity_indicator`, `keyboard_scroll`, `transcript_mode`,
`search`, `resize`, `mouse_release`, `terminal_restore`,
`composer_preserved`; `manual_pending=true`.

`mouse_release` y `terminal_restore` se comprueban por estructura: `MouseGuard`
con `EnableMouseCapture`/`DisableMouseCapture`, `Drop`, `release_mouse()` en el
hook de pánico y `drop(guard)` antes de `ratatui::restore()`. No se observa el
terminal real desde el gate.

## 4. Consola Windows real (parcial)

Ejecuté `hli.exe` en una PTY sobre un `HLI_HOME` limpio y envié un prompt
read-only. El frame final mostró `run.planning` → `run.running` → `Plan completed`
→ `run.completed` → resumen con provider/tokens/receipt.

Limitación: la captura de la PTY llega truncada y no incluye el compositor, así
que **no pude verificar visualmente K5** ni la rueda. No lo presento como
demostrado.

## 5. Pendiente de validación humana en Windows Terminal

- Rueda física y trackpad: fluidez, velocidad y límites.
- Scrollbar visible y proporcionada.
- Subir, generar actividad y confirmar que la vista no salta.
- Contador `↓ N new activity · Ctrl+End`.
- `PgUp`/`PgDn`, `Ctrl+Home`/`Ctrl+End`.
- Transcript Mode, búsqueda, `n`/`N`, `{`/`}`.
- Salida: cursor visible, alternate screen fuera, raw mode off, ratón liberado.
- `HLI_DISABLE_MOUSE=1`: selección nativa y teclado.
- K5 con el compositor visible en pantalla.

## 6. Limitaciones conocidas

- Sin selección interna ni copia con el ratón capturado.
- Sin clic/drag en la scrollbar.
- Sin volcado al scrollback nativo ni export del transcript.
- Un solo renderer.
- La altura desplazable se aproxima a `alto - 9`; en layouts reducidos puede
  diferir en una línea.
- `Ctrl+O` cambia de contrato: ahora abre Transcript Mode; Inspector es `Ctrl+I`.
