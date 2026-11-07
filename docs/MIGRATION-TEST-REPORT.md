# HLI — Informe de pruebas de migraciones

**Commit inicial del bloque:** `2f22b68`
**Commit final:** `feat(store): add versioned transactional migrations`
**Fecha:** 2026-11-07

## Alcance demostrado

| Paso | Resultado |
|---|---|
| Abrir fixture anterior (schema 5) | `current_version=5`, `pending=2` |
| Crear backup | `HLI_HOME/backups/hli-*.db` con versión 5 |
| Aplicar migraciones | `5 → 7`, pasos `[6, 7]` |
| Verificar schema e integridad | `schema_ok`, `integrity_ok` |
| Verificar cadena de eventos | `chain_ok`, 3 eventos, mismo `chain_head` |
| Reiniciar daemon | arranca sobre schema 7 |
| Recuperar estado | workspace, thread, branch, run y receipt |
| Ejecutar run nuevo | `run.start` con `sim` completado |
| Iniciar MCH | `mch serve migration-main` → `ready` |
| Buscar memoria | activa encontrada; superseded excluida |
| Exportar `.hliar` | archivo generado |
| Importar en otro `HLI_HOME` | `verify` OK, memoria y artifact presentes |
| Fallo simulado | versión se mantiene en 5, `migration_failed` |

## Pruebas Rust

`crates/hli-store/tests/migrations.rs` (12):

- `migration_applies_in_order`
- `migration_is_not_reapplied`
- `failed_migration_rolls_back`
- `unsupported_future_schema_is_rejected`
- `backup_is_created_before_apply`
- `migrated_events_preserve_chain`
- `migrated_artifacts_preserve_hashes`
- `migrated_memory_preserves_supersession`
- `migrated_mch_preserves_autostart`
- `daemon_refuses_partial_schema`
- `hliar_future_version_is_rejected`
- `hliar_unknown_version_is_rejected`

`crates/hli-cli/tests/migrations.rs` (3):

- `migration_cli_status_backup_apply_verify`
- `migrated_signing_state_is_valid`
- `future_hlimod_format_is_rejected`

## Gate externo

`scripts/migration_gate.py`:

```json
{
  "gate": "migration",
  "ok": true,
  "previous_fixture": true,
  "backup_created": true,
  "schema_upgraded": true,
  "events_preserved": true,
  "artifacts_preserved": true,
  "memory_preserved": true,
  "mch_preserved": true,
  "rollback_verified": true,
  "export_import_verified": true
}
```

## Suite completa

| Perfil | Pruebas | Fallos |
|---|---|---|
| Debug (`cargo test --workspace`) | 134 | 0 |
| Release (`cargo test --workspace --release`) | 134 | 0 |

Gates previos reejecutados sin regresión: 0.3 (`mcp_gate`), 0.4
(`module_gate`), supervisor MCH y firma de módulos. `cargo audit` y
`cargo deny check` mantienen el resultado anterior (warnings de `lru 0.12.5`
ya justificados).

## Observaciones

- La fixture se genera aplicando solo 0001–0005, por lo que representa el
  esquema real anterior a `autostart` y a la firma, no una copia actual con el
  número de versión cambiado.
- Tras migrar, `mch_instances.autostart` existe con default 0 y puede activarse.
- El estado de firma sigue operativo tras la migración: una clave nueva firma y
  verifica `signed-trusted` sobre el schema 7.
- El fallo simulado (`HLI_MIGRATION_FAIL_AT=6`) no avanza la versión y deja la
  base utilizable.
