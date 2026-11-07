# HLI — Informe de Beta 0.9

**Código inicial del bloque:** `e368ff1` · **Fecha:** 2026-11-07 · **Estado:** Beta en progreso, sin tag.

## Completado en esta etapa

| Bloque | Estado | Evidencia |
|---|---|---|
| Límites de contenedores | hecho | `.hlimod` 256 MiB, `.hliar` 2 GiB, entrada 128 MiB, 10 000 entradas; duplicados y `.`/`..` rechazados; 4 pruebas |
| Proveedor configurable | hecho | `providers.toml`, `provider add/list/inspect/test/set-default/remove`, default persistente, `secret_handle`; 2 pruebas e2e |
| Formato y clippy | verde | `cargo fmt --check`, `cargo clippy -D warnings` |
| Runs asíncronos y cancelación | hecho | `--async`, `status`, `wait`, `cancel`, `list --state`; tabla de transiciones; timeout; recuperación a `interrupted`; 4 pruebas e2e |
| MCH persistente (nucleo) | parcial | proceso reutilizado, lifecycle completo, health periodico, degraded y restart acotado; autostart y fixture pendientes |
| Tests | verde | 85 pruebas; e2e aislados por `HLI_HOME` |

## Pendiente para cerrar 0.9

| Bloque | Estado | Nota |
|---|---|---|
| Gate visual manual | **no ejecutado** | Guía y entorno listos en `docs/TUI-MANUAL-GATE.md`; requiere Windows Terminal |
| Supervisor MCH persistente | parcial | Lifecycle y health periódico hechos; degraded/restart con lógica pero sin fixture de fallo; faltan autostart y pruebas |
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
| MCH persistente | parcial: proceso reutilizado y recuperado como `stopped`; falta autostart |
| Reinicio y recuperación | parcial: `running` → `interrupted`; `queued` no se reencola |
| Export/import | sí (existente) |
| Migración | no |
| TUI operativa | sí en tests de buffer; gate manual pendiente |

## Decisión

**No se crea `v0.9.0-beta.1`.** El gate no se cumple: faltan supervisor persistente, firma, migraciones, paquete y gate visual. Crear el tag ahora sería declarar una beta que no existe.

## Siguiente bloque

1. Completar supervisor MCH: health, degraded, restart automático, autostart y módulo de prueba.
2. Firma Ed25519 y políticas de módulo.
3. Migraciones con fixture real.
4. Empaquetado, licencia, CI y documentación pública.
5. Gate visual manual y gate Beta completo.
