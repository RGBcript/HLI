# Protocolo agéntico: referencias y propuesta para HLI

Fecha: 2026-11-09. Fuentes: documentación de Command Code (permissions, agents, skills,
interactive-mode), AGENTS.md (Agentic AI Foundation), docs de Codex/Gemini/OpenCode/Crush y
repos de sincronización de instrucciones entre CLIs.

## 1. Qué hacen los CLIs de referencia

### Command Code

| Pieza | Cómo funciona |
|---|---|
| Modos de permiso | `default` → `accept-edits` → `plan` → `yolo`, con `shift+tab`. `dont-ask` aparte: **fail-closed** para CI (solo lo pre-aprobado; lo demás se deniega) |
| Reglas | `deny` > `ask` > `allow`, con patrones: `Shell(git status)`, `Edit(src/**)`, `Read(./.env)`, `mcp__github__*` |
| Fast path | Lecturas y shell de solo lectura no preguntan; un comando compuesto cuenta si **todas** sus partes lo son |
| Cortacircuitos | `rm -rf /` y `~` preguntan incluso en `yolo`; escrituras sensibles (`.env`, `.git/**`, `.ssh/**`) preguntan |
| Subagentes | `General`, `Explore` (solo lectura), `Plan`; contexto aislado, modelo propio, un nivel de profundidad, ejecución en background con `agent_id` + `agent_output` |
| Skills | Estándar abierto con *progressive disclosure*: al arrancar solo nombre/descripción; el `SKILL.md` se carga al activarse |
| Instrucciones | `AGENTS.md` global (`~/.commandcode/AGENTS.md`) y por proyecto; `@archivo` mete contexto |
| Sesión | `Esc Esc` rebobina a checkpoint; `/tree` y `/clone` ramifican; `ctrl+s` guarda el prompt; `ctrl+g` lo abre en `$EDITOR` |
| Contexto | Compactación por umbral; ledger de tareas; panel de background (`Ctrl+B`) |

### Resto de CLIs

- **AGENTS.md es el estándar de facto** (Codex, Cursor, Copilot, Gemini, OpenCode, Crush, Junie,
  Goose). Codex lo concatena desde la raíz hasta el cwd, con tope de 32 KiB por nivel.
- **Memoria**: `CLAUDE.md`/`MEMORY.md` (escrita por el agente) para lo episódico, `AGENTS.md` para lo
  semántico, skills para lo procedural. La compactación llega al 86–92 % del contexto.
- **Skills** en `.agents/skills/` son portables entre herramientas.
- **Aislamiento**: subagentes y worktrees para trabajo paralelo sin contaminar el contexto.
- **Permisos**: modos + listas deny/ask/allow; el modo `plan` solo puede escribir el archivo de plan.

## 2. Qué tiene ya HLI

| Capacidad | Estado |
|---|---|
| Event log append-only, snapshots, receipts | Sí |
| Workspace → thread → branch, runs asíncronos | Sí |
| Plan durable con efectos y aprobación | Sí |
| Permiso por invocación (`once`/`run`/`workspace`) | Sí |
| `@archivo` explícito | Sí |
| Memoria temporal y supersesión | Sí |
| MCP, MCH, módulos firmados | Sí |
| System prompt | Fijo, sin `AGENTS.md` |
| Modos de permiso | No: solo la política por tool |
| Subagentes | No |
| Skills | No |
| Checkpoints/rewind en UI | No (hay snapshots internos) |
| Compactación de contexto | No: `budget_tokens` fijo |

## 3. Propuesta para HLI

### Estado

- `AGENTS.md` en la raíz del repo: **aprobado y creado**. Sirve a los agentes que trabajen *en* HLI.
- Falta el paso de runtime: que **HLI lea** `AGENTS.md` como instrucciones de proyecto y las selle en
  el `ContextManifest`. Es el punto 1 de P0 y sigue pendiente.

### P0 — lo que desbloquea el uso real

1. **`AGENTS.md` jerárquico.** Leer `HLI_HOME/AGENTS.md`, `<workspace>/AGENTS.md` y los de los
   subdirectorios hasta el cwd (tope 32 KiB por nivel, como Codex), y sellarlos en el
   `ContextManifest` con hash. Sustituye el `SYSTEM_PROMPT` fijo por identidad + instrucciones del
   proyecto. Es la pieza que hoy provoca respuestas como “HLI = Human Language Interface”.
2. **Modos de permiso.** `default` / `accept-edits` / `plan` / `dont-ask`, con `Shift+Tab` y
   `/mode`. `dont-ask` es el modo CI: solo lo pre-aprobado, lo demás denegado. `yolo` no se
   implementa: HLI deniega `shell.execute` por diseño y no debe existir un bypass.
3. **Reglas con patrón.** Extender el target exacto a globs: `Edit(src/**)`, `Read(./.env)`,
   `Shell(git status)`. Orden deny > ask > allow, ya probado en el motor de permisos.
4. **`!comando` de solo lectura.** `shell.execute` está denegado; permitir `!` únicamente con una
   allowlist de comandos de lectura (`ls`, `cat`, `git status`, `rg`) y salida al transcript.
5. **Skills.** `.agents/skills/<name>/SKILL.md` con frontmatter (name, description), cargadas por
   *progressive disclosure* y referenciadas desde `AGENTS.md`. Reutiliza el estándar, no inventa.

### P1 — calidad de sesión

6. **Checkpoints/rewind.** Ya hay snapshots y event log; falta `Esc Esc` y `/rewind` que muevan el
   branch a una revisión anterior sin perder el historial.
7. **Compactación de contexto.** Umbral (80–90 % de `budget_tokens`), resumen durable como evento y
   recarga del manifiesto. Hoy el presupuesto es fijo y no se adapta.
8. **Subagente `Explore`.** Un run hijo de solo lectura, con su propio plan y contexto, que devuelve
   un resumen. Encaja con thread/branch; no requiere un runtime nuevo.
9. **`Ctrl+G` y `Ctrl+S`.** Editor externo para el prompt y borrador persistente (draft).

### P2 — después de publicar

10. `/tree` y `/clone` sobre branches existentes.
11. Memoria procedural por workspace (`MEMORY.md` escrita por el agente, con supersesión).
12. Panel de trabajos en background (MCH ya tiene procesos; falta la vista).

### Lo que no se copia

- `yolo` / `--dangerously-skip-permissions`: contradice la política de HLI.
- Aprendizaje de “taste”: es un producto, no un protocolo.
- Automatización de web chats: fuera del core por spec.

## 4. Orden de implementación sugerido

1. `AGENTS.md` + manifiesto (1–2 días, sin tocar el motor).
2. Modos + reglas con glob (1 día, sobre el motor de permisos actual).
3. `!` de solo lectura y skills (1 día).
4. Checkpoints y compactación (2 días).
5. Subagente `Explore` (2–3 días).

Cada punto debe llegar con su gate: `AGENTS.md` con hash en el manifiesto; modos con tabla de
decisión probada; `!` con allowlist y caso negativo; skills con carga diferida verificada.
