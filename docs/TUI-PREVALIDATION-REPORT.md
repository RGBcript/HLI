# HLI — Informe de prevalidación de la TUI

**Commit evaluado:** `bf101bd` · **Versión:** `0.9.0-beta.1` · **Fecha:** 2026-11-09
**Estado:** prevalidación. **No sustituye** la firma humana de
`docs/AGENT-TUI-MANUAL-GATE.md`.

---

## 1. Qué se ejecutó

| Arnés | Alcance | Resultado |
|---|---|---|
| `cargo test -p hli-tui --test render` | 134 pruebas de buffer: teclas, foco, layouts, overlay de plan, overlay de permiso, Inspector, transcript | **134/134** |
| `scripts/agent_tui_gate.ps1` → `agent_tui_gate.py` | recorrido funcional completo sobre un `HLI_HOME` limpio | **20/20** campos automáticos, `manual_pending=true` |
| `cargo test --workspace` (debug y release) | 36 suites | **278/278** |

## 2. Comprobaciones de teclado y estado

- `Press` + `Release` produce una sola acción; `Release` aislado no actúa.
- `Repeat` se acepta para entrada de texto.
- `Tab` avanza exactamente un foco y vuelve al inicio tras el ciclo.
- `?` abre y cierra la ayuda sin cambiar el estado del compositor.
- Teclear `abc` produce `abc`.
- El feedback de comando permanece visible hasta que se sustituye, se limpia o
  expira.
- Los overlays (plan y permiso) consumen el teclado: el compositor no envía
  prompts mientras están activos.
- `Ctrl+O` conserva el overlay al volver de Inspector.

## 3. Ciclo de terminal

El arnés de buffer no puede observar el terminal real. En esta sesión **no se
re-ejecutó** el arnés PTY de consola real; la evidencia disponible es
`docs/TUI-PTY-EVIDENCE.md` (commit `7a53f12`), que documenta alternate screen,
cursor y `exit=0`. Repetirla sobre `bf101bd` queda como tarea del recorrido
humano (Parte D).

## 4. Qué no cubre esta prevalidación

- Percepción visual humana (legibilidad, contraste, ergonomía).
- Restauración real de cursor y raw mode tras un cierre abrupto.
- Comportamiento con terminales distintos de Windows Terminal.
- Firma del gate: `manual_pending` sigue en `true` por diseño.

## 5. Conclusión

La lógica de interacción y los flujos funcionales están cubiertos por pruebas
automáticas. La prevalidación **no autoriza** el tag Beta: falta la firma del
recorrido humano de 30 pasos.
