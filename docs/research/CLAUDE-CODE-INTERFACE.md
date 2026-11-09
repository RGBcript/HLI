# Claude Code: inventario completo de la interfaz

**Investigación:** 2026-11-09 · **Fuentes oficiales:** `code.claude.com/docs/en/`
(fullscreen, interactive-mode, keybindings, permissions, accessibility,
checkpointing, commands, statusline, agent-view).
**Alcance:** comportamiento observable de la interfaz de terminal. No se copian
textos, branding ni implementación; se documenta para decidir qué adaptar en HLI.

---

## 1. Superficies

Claude Code no es una sola pantalla. Tiene seis superficies con contratos
distintos:

| Superficie | Entrada | Qué es |
|---|---|---|
| Prompt / REPL clásico | `claude` (renderer classic) | Conversación en el scrollback nativo; el compositor se desplaza con el output |
| Fullscreen | `/tui fullscreen` o por defecto según instalación | Alternate screen; compositor fijo abajo; sólo se dibuja lo visible |
| Transcript viewer | `Ctrl+O` | Revisión y búsqueda de la conversación a pantalla completa |
| Diff panel | `/diff` (panel lateral) | Cambios acumulados mientras el agente trabaja |
| Agent view | `claude agents` | Lista de sesiones en segundo plano: corriendo, esperando input, terminadas |
| Statusline | script configurado | Barra inferior con datos de sesión (modelo, contexto, coste, git) |

A esto se añaden los **paneles de diálogo** (`/config`, `/model`, `/theme`,
`/permissions`, `/mcp`, `/skills`, `/plugin`, `/help`, `/rewind`) y el **panel de
subagentes** bajo el prompt.

## 2. Anatomía de la pantalla fullscreen

```
┌ cabecera / estado de sesión ─────────────────────────────┐
│ (modelo, modo de permisos, effort, fast, contexto)       │
├ transcript (área desplazable) ───────────────────────────┤
│ mensajes, tool calls, diffs, receipts                    │
│ … scrollbar en el borde cuando hay contenido fuera …     │
├ prompt fijo ─────────────────────────────────────────────┤
│ > compositor (multilínea, autocompletado, sugerencias)   │
├ footer ──────────────────────────────────────────────────┤
│ tareas / subagentes / diff / artifacts + statusline      │
└──────────────────────────────────────────────────────────┘
```

Reglas observadas:

- El compositor **no se mueve** mientras llega output: es el rasgo que define
  fullscreen.
- Sólo las líneas visibles se mantienen en el árbol de render; la memoria queda
  plana en conversaciones largas.
- Un header tenue muestra el prompt más reciente que quedó por encima del
  viewport; al hacer clic salta a él.
- Los diálogos que requieren respuesta **siempre** se traen a la vista, aunque
  el usuario esté revisando historia o haya desactivado el auto-scroll.

## 3. Composición de entrada

| Mecanismo | Comportamiento |
|---|---|
| `Enter` | Envía |
| `\` + `Enter`, `Ctrl+J`, `Shift+Enter` (según terminal) | Salto de línea |
| `@` | Autocompletado de rutas y, si hay sesiones vivas, de sesiones |
| `/` | Menú de comandos, skills, plugins y prompts MCP |
| `!` | Modo shell: ejecuta y añade la salida a la sesión |
| `:` | Emoji por nombre corto |
| `?` con input vacío | Ayuda de atajos; con texto, inserta `?` |
| Pegado | Texto y, en terminales compatibles, imágenes como chips `[Image #N]` |
| Historial | `↑`/`↓`, `Ctrl+P`/`Ctrl+N`, búsqueda inversa `Ctrl+R` |
| Draft | `Ctrl+S` guarda/restaura el borrador; doble `Esc` lo limpia y lo manda al historial |
| Cola | Mientras el agente trabaja, los mensajes se encolan; `Up` los recupera; `Ctrl+Enter` los envía ya |
| Sugerencias | Texto gris al abrir y tras cada respuesta; `Tab`/`→` la acepta |
| Limpieza | Se eliminan caracteres invisibles (bidi, zero-width) antes de enviar |
| Ortografía | Subrayado opcional; ignora código, rutas, URLs y flags |

El prompt es un **editor**: `Ctrl+A/E`, `Ctrl+K/U/W`, `Ctrl+Y`, `Alt+B/F/D`,
deshacer con `Ctrl+_`; con `editorMode: vim`, modos NORMAL/INSERT/VISUAL,
objetos de texto y remapeos de secuencias.

## 4. Navegación y lectura

| Entrada | Acción |
|---|---|
| Rueda / trackpad | Desplaza unas líneas; velocidad configurable (`CLAUDE_CODE_SCROLL_SPEED`, `/scroll-speed`) |
| `PgUp` / `PgDn` | Media pantalla |
| `Ctrl+Home` / `Ctrl+End` | Inicio / final con auto-follow |
| `j`/`k`, `↑`/`↓` (transcript) | Una línea |
| `Ctrl+U`/`Ctrl+D` | Media página |
| `Ctrl+B`/`Ctrl+F`, `Space`/`b` | Página completa |
| `g`/`G`, `Home`/`End` (transcript) | Inicio / final |
| `{`/`}` | Prompt anterior / siguiente |
| `/` (transcript) | Búsqueda; `Enter` acepta, `Esc` cancela y restaura posición |
| `n`/`N` | Siguiente / anterior coincidencia |
| `[` (transcript) | Vuelca la conversación al scrollback nativo |
| `v` (transcript) | Vuelca a archivo temporal y lo abre en `$EDITOR` |
| `Ctrl+O`, `Esc`, `q` | Salir del transcript |

**Auto-follow:** al subir se pausa; el contenido nuevo no arrastra la vista. Un
botón flotante `Jump to bottom` muestra `N new messages`. Se reanuda con clic,
`Ctrl+End` o rueda hasta el final. En `/config` puede desactivarse el auto-scroll
por completo, pero los diálogos siguen trayéndose a la vista.

**Selección:** con el ratón capturado, la selección es interna y se copia al
portapapeles (pbcopy / wl-copy / xclip / Set-Clipboard / OSC 52). Para la
selección nativa se mantiene `Shift` (u `Option`/`Fn` según terminal) al
arrastrar. En modo screen reader se escribe texto plano.

## 5. Diálogos y decisiones

- **Select / multiselect:** clic o teclado; rueda dentro del panel; scrollbar en
  listas largas; filas `↑ N more` / `↓ N more` para saltar extremos.
- **`/config`, `/model`, `/theme`, `/mcp`, `/skills`, `/plugin`:** paneles con
  listas, valores editables y descripción al pie.
- **Prompt de permisos:** título de la herramienta, descripción, comando o diff,
  aviso `This command requires approval` y opciones: *Yes*; *Yes, and don't ask
  again for …*; *Yes, and switch to auto mode*; *No*. `Esc` cancela, `Tab` abre
  un campo de comentario en la opción enfocada.
- **`/rewind`:** lista de prompts; acciones *Restore code and conversation*,
  *Restore conversation*, *Restore code*, *Summarize from here*, *Summarize up to
  here*, *Never mind*.
- **`/help`, `?`:** panel de atajos, con contexto.
- **Confirmaciones destructivas:** doble pulsación o confirmación explícita
  (por ejemplo `Ctrl+D` dos veces en 800 ms).

## 6. Modelo de permisos (UI)

| Tipo de herramienta | Aprobación | "No volver a preguntar" |
|---|---|---|
| Lectura | No, dentro del directorio de trabajo y adicionales | — |
| Bash | Sí, salvo comandos de lectura preaprobados | Permanente por repositorio y comando |
| Edición de archivos | Sí | Hasta el fin de sesión |
| WebFetch | Sí, salvo dominios de documentación | Permanente por repositorio y dominio |
| WebSearch | Sí | Permanente por repositorio |

Modos: `default` (Manual), `acceptEdits`, `plan`, `bypassPermissions`, `auto`
(clasificador). Se ciclan con `Shift+Tab` (o `Alt+M` en Windows sin VT). Las
reglas viven en settings y se pueden versionar; las reglas permanentes se
escriben en `.claude/settings.local.json`.

## 7. Estados, feedback y statusline

- Spinner y progreso en vivo; con `prefersReducedMotion` se reducen o eliminan.
- Toast tras copiar, indicando qué vía usó.
- Campana de terminal opcional (`preferredNotifChannel: terminal_bell`) cuando el
  agente espera algo.
- Fila de tareas/subagentes bajo el prompt; `Ctrl+T` muestra/oculta el checklist.
- Statusline: script propio que recibe JSON por stdin y se re-ejecuta al llegar
  un mensaje, al terminar `/compact`, al cambiar el modo de permisos o el modo
  vim, y por `refreshInterval`. Debounce de 300 ms; si el script anterior sigue
  corriendo, se cancela.

## 8. Accesibilidad

- Screen reader mode (`--ax-screen-reader`, `CLAUDE_AX_SCREEN_READER`,
  `axScreenReader`): texto plano, sin cajas, sin señales sólo por color, sin
  redibujados innecesarios; tablas como `Header: value`; diffs con `+`/`-`.
- `CLAUDE_CODE_ACCESSIBILITY=1`: cursor visible para magnificadores.
- Temas daltonizados (`dark-daltonized`, `light-daltonized`).
- `prefersReducedMotion`.
- Límites conocidos: no se activa solo, no anuncia cambios de modo, el
  scrollback no existe en sesiones adjuntas, coste sólo al salir.

## 9. Personalización

- `~/.claude/keybindings.json` con **contextos**: Global, Chat, Autocomplete,
  Settings, Confirmation, Tabs, Help, Transcript, HistorySearch, Task,
  ThemePicker, Attachments, Footer, MessageSelector, DiffDialog, DiffPanel,
  ModelPicker, EffortSlider, Select, Plugin, AbovePrompt, AbovePromptInput,
  AbovePromptSelect, Pane, PaneField, Agents, Scroll.
- Acciones con espacio de nombres (`chat:submit`, `scroll:bottom`,
  `transcript:exit`, `confirm:yes`…), validación con avisos y conflictos
  reservados.
- Statusline y `subagentStatusLine` por script.
- Temas y `editorMode`.

## 10. Sesiones y trabajo paralelo

- `--resume` / `/resume`, `/branch`, `/fork`, `/clear`, `/compact`.
- Checkpoints por prompt con `/rewind`; snapshots de los 100 más recientes.
- `claude agents`: lista de sesiones en segundo plano, con estado
  (`Needs input`, corriendo, terminada); adjuntar y volver con `←`.
- `/background` mueve la sesión actual a segundo plano.
- `/diff`, `/code-review`, `/security-review` antes de publicar.

## 11. Salida y restauración

- `Ctrl+C` interrumpe; con el prompt vacío, una pulsación limpia y otra sale.
- `Ctrl+D` sale (doble pulsación en 800 ms) cuando el prompt está vacío.
- `/exit`, `q` en transcript.
- Al salir: restore del alternate screen, cursor visible, raw mode off, captura
  de ratón liberada.
- Fallos de arranque del fullscreen se cuentan por versión; tras dos, cae al
  classic y lo explica.

## 12. Atajos por contexto (resumen)

| Contexto | Atajos representativos |
|---|---|
| Global | `Ctrl+C`, `Ctrl+D`, `Ctrl+O`, `Ctrl+R`, `?` |
| Chat | `Enter`, `\`+`Enter`, `Ctrl+J`, `Tab` (autocompletado), `Ctrl+S` (stash), `Ctrl+G` (editor), `Ctrl+Enter` (enviar cola) |
| Edición | `Ctrl+A/E/K/U/W/Y`, `Alt+B/F/D`, `Ctrl+_` |
| Autocomplete | `Tab`, `Esc`, `↑`/`↓` |
| Confirmación | `Enter`, `Esc`, `↑`/`↓`, `Tab` (comentario), `Space` (toggle) |
| Transcript | `/`, `n`/`N`, `j`/`k`, `g`/`G`, `{`/`}`, `Ctrl+U/D/B/F`, `[`, `v`, `q`/`Esc` |
| Scroll | rueda, `PgUp/PgDn`, `Ctrl+Home/End`, `scroll:bottom` reasignable |
| Tareas | `Ctrl+B` (background), `Ctrl+T` (checklist) |
| Agentes | navegación de filas, `←` para volver, adjuntar |

## 13. Principios de diseño que HLI puede adoptar

1. **El compositor es sagrado:** fijo, nunca se desplaza, nunca se limpia sin
   confirmación durable.
2. **La conversación es un documento, no un log:** viewport propio, scrollbar,
   posición estable y follow explícito.
3. **El usuario manda en la lectura:** subir pausa el follow; las novedades se
   cuentan, no arrastran.
4. **Las decisiones no se pierden:** un diálogo pendiente se trae a la vista
   aunque el usuario esté en historia.
5. **Transcript y administración son superficies distintas.**
6. **Todo lo que se ve se puede buscar;** si no está en el scrollback, se vuelca
   o se exporta.
7. **El ratón es opcional, no obligatorio:** siempre hay ruta de teclado y una
   forma de devolver la selección al terminal.
8. **La accesibilidad es un renderer, no un tema.**
9. **El estado vive fuera de la UI:** reanudar reconstruye, no duplica.
10. **La terminal se restaura pase lo que pase.**

## 14. HLI frente a Claude Code

| Capacidad | Claude Code | HLI hoy |
|---|---|---|
| Renderer | classic + fullscreen | sólo fullscreen (alternate screen) |
| Compositor fijo | sí | sí |
| Rueda del ratón | sí, velocidad configurable | sí (`HLI_WHEEL_SCROLL_SPEED`, 1–20) |
| Scrollbar | sí, con clic/drag en listas | sí, visual (sin clic/drag) |
| Auto-follow + contador | sí, botón flotante | sí, indicador en la barra de estado |
| `PgUp/PgDn`, `Ctrl+Home/End` | sí | sí |
| Transcript viewer | `Ctrl+O` | `Ctrl+O` (`TuiMode::Transcript`) |
| Búsqueda | `/`, `n`/`N` | `/`, `n`/`N`, resaltado |
| Salto entre prompts | `{`/`}` | `{`/`}` |
| Volcado al scrollback | `[` | **no** |
| Export a archivo/editor | `v` | **no** |
| Selección interna y copia | sí | **no**; `HLI_DISABLE_MOUSE=1` |
| Clic/drag en scrollbar | sí | **no** |
| Expandir tool por clic | sí | sólo `Ctrl+E` |
| Diff panel lateral | `/diff` | **no** |
| Statusline por script | sí | barra fija |
| Keybindings configurables | sí, por contexto | **no** |
| Screen reader mode | sí | **no** |
| Temas daltonizados | sí | limitado (mono/ASCII) |
| Agent view / sesiones en segundo plano | sí | no (MCH es otra cosa) |
| Checkpoints / rewind | sí | no |
| Cola de mensajes y draft | sí | no |
| Sugerencias de prompt | sí | no |
| Vim mode | sí | no |
| Inspector de runs/contexto | no (equivalente parcial: `/status`, `/context`) | **sí** |

## 15. Qué no conviene copiar

- La duplicación de renderers si implica dos motores de UI.
- La captura de ratón sin una vía clara de selección nativa.
- El volcado al scrollback a mitad de sesión sin prueba en Windows Terminal.
- Atajos que chocan con la edición del compositor (`Home`/`End` simples).
- Un número creciente de contextos de keybindings sin necesidad real.

## Anexo — Fuentes

- `https://code.claude.com/docs/en/fullscreen`
- `https://code.claude.com/docs/en/interactive-mode`
- `https://code.claude.com/docs/en/keybindings`
- `https://code.claude.com/docs/en/permissions`
- `https://code.claude.com/docs/en/accessibility`
- `https://code.claude.com/docs/en/checkpointing`
- `https://code.claude.com/docs/en/commands`
- `https://code.claude.com/docs/en/statusline`
- `https://code.claude.com/docs/en/agent-view`

No verificado en detalle: composición interna del diff panel y lista completa de
comandos (la página los trunca en la extracción). El resto se contrastó con el
texto oficial.
