# HLI — Informe de Beta 0.9

**Código:** `7e62c9e` · **Fecha:** 2026-11-07 · **Estado:** Beta en progreso, sin tag.

## Completado en esta etapa

| Bloque | Estado | Evidencia |
|---|---|---|
| Límites de contenedores | hecho | `.hlimod` 256 MiB, `.hliar` 2 GiB, entrada 128 MiB, 10 000 entradas; duplicados y `.`/`..` rechazados; 4 pruebas |
| Proveedor configurable | hecho | `providers.toml`, `provider add/list/inspect/test/set-default/remove`, default persistente, `secret_handle`; 2 pruebas e2e |
| Formato y clippy | verde | `cargo fmt --check`, `cargo clippy -D warnings` |
| Tests | verde | 73 + 2 nuevas = 75 pruebas |

## Pendiente para cerrar 0.9

| Bloque | Estado | Nota |
|---|---|---|
| Gate visual manual | **no ejecutado** | Guía y entorno listos en `docs/TUI-MANUAL-GATE.md`; requiere Windows Terminal |
| Runs asíncronos y cancelables | pendiente | `run start` sigue siendo síncrono; faltan `status/wait/cancel`, estados `cancelling/cancelled/timed_out/interrupted` y recuperación tras reinicio |
| Supervisor MCH persistente | pendiente | El modelo actual crea un proceso por invocación; faltan pool, health, reinicio y recuperación |
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
| Run asíncrono | no |
| Cancelación | no |
| Run completo | sí (síncrono) |
| Módulo firmado o permitido | no |
| MCH persistente | no |
| Reinicio y recuperación | parcial |
| Export/import | sí (existente) |
| Migración | no |
| TUI operativa | sí en tests de buffer; gate manual pendiente |

## Decisión

**No se crea `v0.9.0-beta.1`.** El gate no se cumple: faltan runs asíncronos, supervisor persistente, firma, migraciones y el gate visual. Crear el tag ahora sería declarar una beta que no existe.

## Siguiente bloque

1. Runs asíncronos con `run start --async`, `run status`, `run wait`, `run cancel` y recuperación de runs interrumpidos.
2. Supervisor MCH persistente con health y reinicio acotado.
3. Firma Ed25519 y políticas de módulo.
4. Migraciones con fixture real.
5. Empaquetado, licencia, CI y documentación pública.
