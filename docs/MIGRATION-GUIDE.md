# HLI — Guía de migraciones

**Estado:** migraciones versionadas y transaccionales implementadas.

## Versiones independientes

| Versión | Constante | Dónde se persiste |
|---|---|---|
| Base de datos | `hli_core::versions::DATABASE_SCHEMA_VERSION` = 7 | `schema_migrations` en `hli.db` |
| Eventos | `EVENT_SCHEMA_VERSION` = 1 | columna `events.schema_version` |
| Configuración | `CONFIG_SCHEMA_VERSION` = 1 | `providers.toml`, `keys/index.json`, `trust.json`, `module-policy.json` |
| Formato `.hliar` | `HLIAR_FORMAT_VERSION` = 1 | cabecera `HLIAR1` y `manifest.format_version` |
| Formato `.hlimod` | `HLIMOD_FORMAT_VERSION` = 1 | `spec.format_version` del manifest |

No hay una única constante global: cada formato evoluciona por separado y se
inspecciona en su propio sitio.

## Migraciones SQLite

```
0001_initial          workspaces, threads, branches, events, artifacts,
                      snapshots, runs mínimos, artifact_slices
0002_runs_context     context_manifests
0003_memory_mcp       memories, memories_fts, índices
0004_modules_mch      modules, mch_profiles, mch_instances
0005_async_runs       columnas de runs (branch_revision, state, ended_at,
                      result_json, receipt_json)
0006_mch_autostart    mch_instances.autostart
0007_module_signing   module_signatures
```

Propiedades:

- Se ejecutan en orden estricto; no se permiten saltos.
- Cada paso abre su propia transacción y registra `(version, applied_at, checksum)`
  en `schema_migrations`.
- Una migración ya aplicada no se repite.
- Si un paso falla, su transacción se revierte y la base queda en la versión
  anterior; nunca hay estado parcial.
- Un `schema_migrations` con versión superior a la soportada se rechaza como
  schema futuro.
- `Store::open` aplica pendientes y después llama a `verify_schema`, que exige
  todas las tablas/columnas críticas y `PRAGMA integrity_check`. El daemon no
  arranca sobre un schema parcial.

Las migraciones son Rust versionado (`crates/hli-store/src/migrations.rs`), no
archivos SQL externos; el registro es el mismo.

## CLI

```powershell
hli migrate status
hli migrate backup
hli migrate apply
hli migrate verify
```

`status` devuelve:

```json
{
  "db_exists": true,
  "current_version": 5,
  "target_version": 7,
  "pending": [{"version": 6, "name": "0006_mch_autostart"}, {"version": 7, "name": "0007_module_signing"}],
  "compatible": true,
  "backup_required": true
}
```

- `apply` crea un backup por defecto (`VACUUM INTO`) antes de modificar una base
  existente y devuelve `from_version`, `to_version`, `applied` y `backup`.
- Se niega a migrar con el daemon en marcha: `hli daemon stop` primero.
- `backup` crea una copia consistente en `HLI_HOME/backups/`.
- `verify` comprueba versión, tablas, `integrity_check` y cadena de eventos
  (`chain_ok`, `events_checked`).

## Fixture de versión anterior

`crates/hli-store/src/fixtures.rs` construye una base **versión 5** aplicando
solo las migraciones 0001–0005 y sembrando:

- workspace, thread y branch con tres eventos sellados;
- snapshot con archivo y hash;
- run completado con receipt y `context_manifest`;
- memoria activa y memoria superseded;
- artifact con archivo y hash;
- módulo instalado y perfil/instancia MCH (sin `autostart`).

No es una base actual con el número cambiado: la columna `autostart` no existe y
la migración 0006 es la que la añade. Se regenera con:

```powershell
cargo run -p hli-store --example make_migration_fixture -- <HLI_HOME>
```

## Formatos `.hliar` y `.hlimod`

- `.hliar`: la cabecera `HLIAR<versión>\n` se parsea. Versión actual → aceptar.
  Versión futura → error claro que pide actualizar HLI. Versión desconocida o
  ausente → rechazar. El manifiesto incluye `format_version` y se valida también
  en importación.
- `.hlimod`: `spec.format_version` ausente equivale a 1 (paquetes previos).
  Versión futura → rechazo; versión desconocida → rechazo.
- Una importación no reescribe firmas ni checksums: si el contenido firmado se
  transformara, habría que producir un artefacto derivado con procedencia
  registrada (no ocurre en la importación actual, que copia bytes y valida).

## Configuración

- `providers.toml` lleva `config_version`; al guardar se fija en 1. Una versión
  futura impide arrancar el daemon con un error claro.
- `keys/index.json`, `trust.json` y `module-policy.json` llevan `version`.
  Versiones futuras se rechazan (o, en política, se falla cerrado en
  `require-signed`).
- Los backups y exportaciones solo contienen metadatos y referencias: los
  secretos viven en `HLI_HOME/secrets/` y nunca se copian.

## Rollback

Ante un fallo de migración:

1. La transacción del paso fallido se revierte.
2. `schema_migrations` conserva la última versión buena.
3. La base sigue siendo utilizable en esa versión.
4. El backup previo permite restaurar el archivo completo si hiciera falta.

Para pruebas y gates existe `HLI_MIGRATION_FAIL_AT=<versión>`, que fuerza un
fallo controlado tras aplicar ese paso.

## Limitaciones

- No hay downgrade automático: `apply` rechaza bajar de versión; se restaura el
  backup manualmente.
- Las migraciones no reescriben datos salvo defaults de columnas nuevas.
- Los backups son locales y no se cifran; no incluyen secretos.
