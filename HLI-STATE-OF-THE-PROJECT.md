# HLI — Estado del proyecto

**Versión:** `0.9.0-beta.1` · **Commit:** posterior a `80bee6d` · **Fecha:** 2026-11-09
**Repositorio:** `C:\Users\GUERRA\Documents\TEC\Doc\Software\HLI`
**Plataforma probada:** Windows x86_64. Linux: experimental.

Este documento es la fotografía oficial del sistema en este commit. No describe lo
que HLI podría llegar a ser: describe lo que está implementado, lo que está probado
y lo que falta, con la fuente de cada afirmación.

---

## 1. Objetivo

HLI es un **agent harness local de peso medio, CLI-first, MCP-native y autónomo**.
Convierte sesiones, modelos, herramientas y proveedores heterogéneos en
workspaces persistentes, ramificables y reutilizables: contexto trazable, memoria
con autoridad y supersesión, event log verificable, artefactos con hash, módulos
instalables y procesos especializados supervisados (MCH). Todo vive en un almacén local SQLite y se
expone por tres superficies: CLI, TUI y MCP.

El objetivo de la Beta 0.9 es ser **instalable y auditable**: paquete Windows,
instalador por usuario, firma de módulos, migraciones versionadas y gates que
demuestren cada afirmación. No es un producto terminado ni un servicio.

**Lo que HLI no es** (por diseño, hoy): un sandbox de ejecución, un orquestador
multiagente distribuido, un servicio de red, ni un cliente MCP universal.

---

## 2. Arquitectura actual

```
                    ┌──────────────┐   ┌──────────────┐   ┌──────────────┐
                    │  hli (CLI)   │   │  hli-tui     │   │  hli-mcp     │
                    │  cliente     │   │  cliente     │   │  servidor    │
                    └──────┬───────┘   └──────┬───────┘   └──────┬───────┘
                           │ TCP 127.0.0.1, NDJSON              │
                           └───────────────┬────────────────────┘
                                           ▼
                              ┌─────────────────────────┐
                              │      hli-daemon         │
                              │  dispatch de comandos   │
                              │  supervisor MCH         │
                              │  proveedores            │
                              └───────┬──────────┬──────┘
                                      │          │
                       ┌──────────────▼──┐   ┌───▼──────────────┐
                       │   hli-store     │   │  hli-provider    │
                       │ SQLite + hashes │   │ sim/ollama/openai│
                       │ migraciones     │   └──────────────────┘
                       │ export/import   │
                       └───────┬─────────┘
                               │
                  ┌────────────▼─────────────┐
                  │ hli-core: canon, ids,    │
                  │ error, tiempo, versiones │
                  └──────────────────────────┘

   Procesos hijos del daemon:  módulos .hlimod (JSON-RPC por stdio)
                                instancias MCH (lifecycle, health, reinicios)
   Seguridad: hli-signing (Ed25519, trust store, políticas de firma)
```

Decisiones estructurales vigentes:

- **Un solo escritor**: el daemon es el único proceso que abre `hli.db`. CLI, TUI y
  MCP son clientes.
- **Event log como fuente de verdad**: toda mutación relevante deja evento con
  cadena SHA-256 (`previous_hash`).
- **Contexto inmutable**: el `ContextManifest` se persiste **antes** de llamar al
  modelo (ADR-0006).
- **Procesos fuera del daemon**: módulos y MCH corren como hijos con protocolo
  propio; el daemon los supervisa, no los embebe.

---

## 3. Componentes implementados

| Componente | Qué hace hoy | Evidencia |
|---|---|---|
| Core | Canon JCS, IDs, errores tipados, versiones, tiempo | `crates/hli-core` (685 líneas, 10 pruebas) |
| Store | SQLite, 15 tablas, esquema v7, 7 migraciones, backup, export/import, snapshots | `crates/hli-store` (4.273 líneas, 23 pruebas) |
| Daemon | ~65 comandos, event log, agent loop, runs sync/async, supervisor MCH, proveedores | `crates/hli-daemon` (2.532 líneas, 22 pruebas) |
| Agent | Loop de ejecución, construcción de manifiesto, receipt | `crates/hli-agent` (483 líneas, 7 pruebas) |
| CLI | ~30 comandos, `--output json\|human` | `crates/hli-cli` (909 líneas, 29 pruebas) |
| TUI | 6 pestañas, 5 áreas de foco, 5 layouts, barra de comandos | `crates/hli-tui` (1.291 líneas, 28 pruebas) |
| MCP | 11 tools por stdio, framing Content-Length, límite 8 MiB | `crates/hli-mcp` (322 líneas) |
| Providers | `sim`, Ollama (`/api/chat`), OpenAI-compatible (`/chat/completions`), precios y coste | `crates/hli-provider` (365 líneas, 3 pruebas) |
| Memory (módulo) | `memory.commit/get/search/update/forget`, `context.build` sobre el daemon | `crates/hli-module-memory` (186 líneas) |
| MCH | Instancias persistentes: lifecycle, health, autostart, límite de 3 reinicios por ventana, `process_generation` | `crates/hli-daemon/src/supervisor.rs` |
| Signing | Ed25519, trust store, políticas, estado `signed-trusted` | `crates/hli-signing` (896 líneas, 12 pruebas) |
| Empaquetado | `dist/hli-0.9.0-beta.1-windows-x86_64/`, instalador/desinstalador, checksums | `scripts/package_windows.ps1` |
| CI | Windows: fmt, clippy, tests, audit, deny. Linux: núcleo | `.github/workflows/ci.yml` |

---

## 3.b Planificación durable y aprobación

Implementado y probado:

- `RunPlan` público con revisión, `supersedes_plan_id`, `invalidation_reasons`
  y hash canónico del contenido revisado.
- `ExecSnapshot` privada, durable y hasheada: provider, modelo, endpoint,
  sampling, timeout, manifiesto, revisión, política de herramientas, módulos,
  MCH y handles `id@versión` (nunca valores).
- Estados `planning` y `waiting_approval`; un run con efectos no ejecuta nada
  hasta que se aprueba.
- `run.plan`, `run.approve` (con `plan_id` obligatorio en MCP), `run.reject` y
  `run.replan`; scopes `once`, `run` y `workspace` con grants persistentes.
- Invalidación por cambio material (provider, modelo, endpoint, timeout,
  manifiesto, revisión de rama, módulos, MCH, política o versión de secreto):
  evento `plan.invalidated`, revisión conservada y replan obligatorio.
- Overlay de aprobación en Agent Mode, sección read-only y acciones en
  Inspector, transcript durable y eventos plan.* en JSONL.
- Receipt con procedencia completa (`plan_id`, revisión, hashes, scope,
  eventos, efectos, supersedes, invalidaciones, provider/model/adapter).
- Herramientas MCP `run_plan`, `run_approve`, `run_reject` y `run_replan`, sin
  exponer `execution_json`.
- Gate `scripts/planning_approval_gate.py` con 19 comprobaciones, incluida
  seguridad de secretos y acciones del Inspector.

No implementado todavía: **permisos interactivos por cada tool call**. La
aprobación cubre el conjunto declarado de efectos del plan, no invocaciones
individuales.

---

## 4. Componentes pendientes

| Pendiente | Naturaleza | Bloquea |
|---|---|---|
| Firma del gate visual de la TUI (96 pasos) | Verificación humana, no código | Tag `v0.9.0-beta.1` |
| Permisos interactivos por tool call | Ingeniería nueva (siguiente bloque) | Aprobación granular por invocación |
| Guion del gate visual desalineado con la TUI real | Corrección de documentación | Ejecución del gate |
| Gates de integración en Linux | Portar scripts | Soportar Linux como plataforma |
| Validación con clientes MCP de terceros | Prueba externa | Afirmar compatibilidad MCP |
| Sandbox / ejecución segura de shell | Ingeniería nueva | Uso en producción |
| Routing económico general | Ingeniería nueva | Afirmar ahorro de coste |
| Streaming en la TUI | Ingeniería nueva | UX en runs largos |
| SBOM y auditoría de código de terceros | Proceso | Afirmar cadena de suministro |
| ADRs de firma, migraciones, empaquetado y autostart | Documentación | Trazabilidad de decisiones |

---

## 5. Decisiones importantes

1. **TCP + NDJSON en loopback**, no HTTP/WebSocket (ADR-0001). Sin TLS ni auth
   remota: el modelo de amenaza es usuario local.
2. **`previous_hash` dentro del sobre canónico** (ADR-0002), corrigiendo la
   especificación: si se excluía, la cadena no era verificable.
3. **JCS como subconjunto documentado** (ADR-0003), no RFC completo.
4. **Contenedor `hliar` provisional** (ADR-0004): sin zip/zstd todavía.
5. **Alcance por fases** (ADR-0005): 0.1 solo durabilidad; ejecución entra en 0.2.
6. **Manifiesto inmutable y previo a la llamada** (ADR-0006).
7. **Trayectoria de rama y prompt cubierto** (ADR-0007): el contexto no mezcla
   ramas hermanas.
8. **Protocolo de módulos propio, separado de MCP** (ADR-0008): JSON-RPC por
   stdio, métodos `initialize/capabilities/configure/invoke/cancel/health`.
9. **Autostart con guard por instancia** (bloque 0.9): impide procesos duplicados
   entre autostart, `mch serve` y reinicios.
10. **Un commit por bloque funcional**, gates externos en Python/PowerShell que
    usan binarios reales, no mocks.

---

## 6. ADRs resumidos

| ADR | Decisión | Estado |
|---|---|---|
| 0001 | Transporte local TCP + NDJSON | Aceptada |
| 0002 | `previous_hash` dentro del sobre canónico | Aceptada (corrige spec) |
| 0003 | JCS como subconjunto documentado | Aceptada |
| 0004 | Contenedor `hliar` provisional | Aceptada |
| 0005 | Alcance estricto de 0.1 (solo durabilidad) | Aceptada |
| 0006 | Run, ContextManifest y agent loop de 0.2 | Aceptada |
| 0007 | Trayectoria de rama, prompt cubierto, memoria y MCP | Aceptada |
| 0008 | Module Protocol, `.hlimod` y MCH de 0.4 | Aceptada |

No hay ADR para: firma Ed25519, migraciones versionadas, empaquetado Windows ni
autostart/límite de reinicios. Son decisiones tomadas en 0.5–0.9 sin registro
formal (ver deuda técnica en `HLI-BETA-REVIEW.md`).

---

## 7. Pruebas existentes

**134 pruebas** en el workspace, verdes en debug y release en el gate Beta.

| Crate | Pruebas | Crate | Pruebas |
|---|---:|---|---:|
| hli-cli | 29 | hli-store | 23 |
| hli-tui | 28 | hli-daemon | 22 |
| hli-signing | 12 | hli-core | 10 |
| hli-agent | 7 | hli-provider | 3 |
| hli-mcp | 0 | hli-module-memory | 0 |
| hli-module-fixture | 0 | | |

Lectura honesta: hay cobertura fuerte de CLI, store y TUI; **MCP y los módulos no
tienen pruebas unitarias propias** y dependen de gates externos. Los e2e se aislan
por `HLI_HOME` y lanzan binarios reales.

---

## 8. Gates existentes

| Gate | Archivo | Qué demuestra |
|---|---|---|
| MCP | `scripts/mcp_gate.py` | Cliente externo JSON-RPC, trayectoria de rama, prompt cubierto, memoria en manifiesto, receipt, reconexión, export/import |
| Módulos | `scripts/module_gate.py` | Empaquetar, instalar, probar, MCH, CLI y MCP, reinicio, checksums, SDK, permisos, vigencia |
| Supervisor MCH | `scripts/mch_supervisor_gate.py` | Lifecycle, persistencia, PID, degradación, recuperación, reinicio, cierre limpio, autostart, límite de reinicios |
| Firma | `scripts/module_signing_gate.py` | Clave, trust, firma, instalación, tamper, revocación |
| Migraciones | `scripts/migration_gate.py` | Fixture previa real, backup, apply, verificación, rollback, MCH, memoria, export/import |
| Instalación | `scripts/install_gate.ps1` | Instalación limpia sobre el paquete Windows real |
| Beta | `scripts/beta_gate.ps1` | Agrega todo lo anterior + fmt, clippy, tests debug/release, audit, deny, empaquetado |

`beta_gate.ps1` devuelve `ok=false` mientras `tui_manual` sea `false`. Ese valor
está fijado en el script y solo cambia cuando el usuario firma
`docs/TUI-MANUAL-GATE.md`.

---

## 9. Limitaciones conocidas

**Verificación**

- El gate visual de la TUI no se ha ejecutado ni firmado.
- El guion de ese gate **no coincide con el código** en tres puntos verificados
  por PTY el 2026-11-07: el ciclo de foco (paso 4), la secuencia tras llegar a la
  barra de comandos (paso 5) y el umbral de layout compacto (paso 13).
- Linux no tiene portados los gates de integración.
- MCP se probó con un cliente propio, no con clientes de terceros.
- Sin SBOM ni auditoría de código de dependencias.

**Producto**

- `shell.execute` está denegado; no hay sandbox de SO.
- `artifact.put` no confina la ruta de origen.
- `read_container`/`import` no limitan tamaño.
- La TUI no hace streaming: polling de 500 ms / 1 s / 4 s.
- Preferencias TOML no implementadas; `Alt+1..6` documentado pero no implementado.
- Sin routing económico general ni compatibilidad MCP universal.

**Dependencias**

- `paste 1.0.15` (unmaintained) y `lru 0.12.5` (dos `unsound`) llegan por
  `ratatui 0.29`; no se actualizan sin cambiar de rama de ratatui. Aceptados y
  justificados en `deny.toml`.

---

## 10. Mapa del sistema

```
HLI  (0.9.0-beta.1 · 6f273be)
├── Core            crates/hli-core
├── Store           crates/hli-store
├── Daemon          crates/hli-daemon
├── CLI             crates/hli-cli
├── TUI             crates/hli-tui
├── MCP             crates/hli-mcp
├── Providers       crates/hli-provider
├── Memory          crates/hli-module-memory
├── MCH             crates/hli-daemon/src/supervisor.rs
├── Module SDK      crates/hli-signing + docs/MODULE-*.md
└── Security        crates/hli-signing + SECURITY.md + THREAT-MODEL.md
```

| Componente | Estado | Pruebas | Gates | ADRs | Archivos |
|---|---|---|---|---|---|
| Core | Implementado | 10 | vía todos | 0002, 0003 | `canon.rs`, `error.rs`, `ids.rs`, `model.rs`, `time.rs`, `versions.rs` |
| Store | Implementado | 23 | migraciones, beta | 0004 (formato), 0005 | `schema.rs`, `migrations.rs`, `writer.rs`, `reader.rs`, `export.rs`, `fixtures.rs` |
| Daemon | Implementado | 22 | mcp, módulos, MCH, migraciones | 0001, 0006, 0007, 0008 | `main.rs`, `supervisor.rs`, `providers.rs` |
| CLI | Implementado | 29 | todos los gates lo usan | — | `main.rs` (909 líneas) |
| TUI | Implementado, gate manual pendiente | 28 | ninguno automático | — | `app.rs`, `ui.rs`, `client.rs`, `main.rs` |
| MCP | Implementado, alcance básico | 0 | mcp_gate | 0007 | `main.rs` (11 tools) |
| Providers | Implementado, requiere entorno | 3 | indirecto (runs) | — | `lib.rs` |
| Memory | Implementado como módulo | 0 | module_gate, mcp_gate | 0008 | `main.rs`, manifiesto `>=0.4 <0.5` |
| MCH | Implementado | 22 (daemon) | mch_supervisor_gate | 0008 | `supervisor.rs`, `docs/MCH-*.md` |
| Module SDK | Implementado para 0.4 | 12 (signing) | module_gate, signing_gate | 0008 | `hli-signing/src/*`, `docs/MODULE-*.md` |
| Security | Parcial: firma y rutas, sin sandbox | 12 | signing_gate, audit, deny | — | `SECURITY.md`, `THREAT-MODEL.md`, `docs/KEY-MANAGEMENT.md` |

Documentación existente: 57 archivos versionados en `docs/` (arquitectura, CLI,
MCP, módulos, MCH, proveedores, migraciones, empaquetado, auditorías 0.1–0.5,
informes de benchmark y de release).

---

## 11. Roadmap restante

**Para cerrar la Beta (corto plazo)**

1. Corregir `docs/TUI-MANUAL-GATE.md` (pasos 4, 5 y 13) para que describa la TUI
   real; o corregir la TUI si el comportamiento esperado era el documentado.
2. Ejecutar los 19 pasos en Windows Terminal y firmar la sección 6.
3. Ajustar `beta_gate.ps1` para que `tui_manual` dependa de esa firma, no de una
   constante.
4. Crear el tag `v0.9.0-beta.1` y publicar el paquete ya construido.

**Para 1.0 (medio plazo)**

5. Portar los gates de integración a Linux y declarar plataforma soportada.
6. Validar con al menos un cliente MCP de terceros.
7. Resolver o documentar formalmente los avisos de `lru`/`paste`.
8. Escribir los ADRs de firma, migraciones, empaquetado y autostart.
9. SBOM y revisión de dependencias.

**Fuera de 1.0 (largo plazo)**

10. Sandbox real o eliminación definitiva de `shell.execute`.
11. Routing económico con medición de coste.
12. Streaming en la TUI y preferencias persistentes.
13. Compatibilidad MCP amplia (recursos, prompts, transporte HTTP).

---

## 12. Cómo mantener este documento

Actualizarlo **solo** cuando cambie el commit de referencia o el estado de un
gate. Cada afirmación debe poder rastrearse a un archivo, una prueba o una salida
de gate. Si algo no se puede verificar, va a "No probado" en
`HLI-BETA-REVIEW.md`, no aquí.

---

## 3.c Permisos por invocación (esquema 9)

- `ToolPermissionRequest` durable: run, plan y revisión, tool, operación, efecto,
  target normalizado, `arguments_hash`, resumen sin secretos y decisión.
- `ToolGrant` por invocación con scopes `once | run | workspace`; `once` no se
  persiste, se consume al decidir.
- `runs.approval_kind` distingue `plan` de `tool`.
- Política: deny explícito → deny; grant válido → allow; sin regla → ask;
  `shell.execute` siempre denegado.
- Daemon: `permission.list/get/approve/deny/cancel`; al aprobar se verifica el
  hash y se ejecuta exactamente la invocación congelada.
- TUI: overlay `Permission required`, detalles y acciones; Inspector con la
  solicitud y su decisión.
- MCP: `permission_list/get/approve/deny`, sin `invocation_json`.
- Receipt: decisiones, solicitudes, invocaciones y grants once consumidos.
- Gate `scripts/tool_permission_gate.py`.

Pendiente conocido: el caso «herramienta no declarada» (`plan.revised` durante
la ejecución) no es reproducible con el proveedor `sim`; el código existe, pero
el gate no puede demostrarlo sin un proveedor que emita una tool fuera del plan.
