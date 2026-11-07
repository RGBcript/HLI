# HLI — Informe de Beta 0.9

**Código inicial del bloque:** `e368ff1` · **Fecha:** 2026-11-07 · **Estado:** Beta en progreso, sin tag.

## Completado en esta etapa

| Bloque | Estado | Evidencia |
|---|---|---|
| Límites de contenedores | hecho | `.hlimod` 256 MiB, `.hliar` 2 GiB, entrada 128 MiB, 10 000 entradas; duplicados y `.`/`..` rechazados; 4 pruebas |
| Proveedor configurable | hecho | `providers.toml`, `provider add/list/inspect/test/set-default/remove`, default persistente, `secret_handle`; 2 pruebas e2e |
| Formato y clippy | verde | `cargo fmt --check`, `cargo clippy -D warnings` |
| Runs asíncronos y cancelación | hecho | `--async`, `status`, `wait`, `cancel`, `list --state`; tabla de transiciones; timeout; recuperación a `interrupted`; 4 pruebas e2e |
| MCH persistente (nucleo) | hecho | proceso reutilizado, lifecycle completo, health periódico, degraded/recovery, restart acotado, autostart y límite de 3 reinicios por ventana; 20 pruebas | 
| Tests | verde | 105 pruebas en debug y 105 en release; e2e aislados por `HLI_HOME` |

## Pendiente para cerrar 0.9

| Bloque | Estado | Nota |
|---|---|---|
| Gate visual manual | **no ejecutado** | Guía y entorno listos en `docs/TUI-MANUAL-GATE.md`; requiere Windows Terminal |
| Supervisor MCH persistente | **cerrado** | Lifecycle, persistencia, degraded/recovery, autostart, límite de reinicios y recuperación manual demostrados por gate y 20 pruebas Rust |
| Firma de módulos Ed25519 | pendiente | Sin `signature.json`, trust store ni políticas `allow/warn/require` |
| Migraciones | pendiente | Sin `migrate status/apply/backup/verify` ni fixture de versión anterior |
| Empaquetado Windows | pendiente | Sin `install.ps1`, `uninstall.ps1`, `dist` completo ni checksums |
| Documentación pública | parcial | `PROVIDER-GUIDE.md` y `TUI-MANUAL-GATE.md`; faltan el resto |
| Licencia Apache 2.0 | pendiente | No se declara hasta tener LICENSE/NOTICE e inventario |
| CI Windows/Linux | pendiente | Sin workflow reproducible |

## Gate 0.9

| Paso | Estado |
|---|---|
| Instalación limpia | no |
| Daemon | sí |
| Provider real | configurable; probado con `sim`; Ollama/OpenAI dependen del entorno |
| Workspace/thread/branch | sí |
| Run asíncrono | sí |
| Cancelación | sí |
| Run completo | sí (síncrono) |
| Módulo firmado o permitido | no |
| MCH persistente | sí: proceso reutilizado, autostart y límite de reinicios |
| Reinicio y recuperación | parcial: `running` → `interrupted`; `queued` no se reencola |
| Export/import | sí (existente) |
| Migración | no |
| TUI operativa | sí en tests de buffer; gate manual pendiente |

## Decisión

**No se crea `v0.9.0-beta.1`.** El gate no se cumple: faltan supervisor persistente, firma, migraciones, paquete y gate visual. Crear el tag ahora sería declarar una beta que no existe.

## Siguiente bloque

1. Firma Ed25519 y políticas de módulo (no iniciada en esta sesión).
2. Migraciones con fixture real.
3. Empaquetado, licencia, CI y documentación pública.
4. Gate visual manual y gate Beta completo.

## Cierre del supervisor MCH (2026-11-07)

| Dato | Valor |
|---|---|
| Commit inicial | `8bfdab1` |
| Commit final | `feat(mch): enforce bounded automatic restarts` |
| Pruebas debug | 105 passed, 0 failed |
| Pruebas release | 105 passed, 0 failed |
| Gate 0.3 (`mcp_gate.py`) | `ok: true` |
| Gate 0.4 (`module_gate.py`) | `ok: true` |
| Gate supervisor (`mch_supervisor_gate.py`) | `ok: true` con las 9 fases |
| PIDs observados | 10980 (inicial) → 21880 (restart) → 23476 (reset manual) → 12036, 20812, 13896 (automáticos) |
| Generaciones | 1 → 2 → 3 → 4, 5, 6; recuperación manual 7; autostart 1 |
| Límite alcanzado | `failed`, `pid=null`, `restart_count_in_window=3`, `last_error.code=restart_limit_exceeded` |
| Recuperación manual | `mch restart` → `ready`, contador y ventana limpios, generación incrementada |
| Autostart preservado | tras reiniciar el daemon la instancia vuelve a `ready` y la memoria previa se recupera |
| Procesos huérfanos | ninguno (`hli-daemon`/`hli-module-fixture` cerrados) |
| Limitación restante | los timers del limitador no se persisten entre reinicios del daemon; la protección aplica dentro de la vida del daemon |
