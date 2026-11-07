# ADR-0004 — Contenedor `hliar` provisional

**Estado:** aceptada (0.1) · **Fecha:** 2026-11-07

## Contexto

El Apéndice D/§15.9 describe `hli-archive/v1` con `manifest.json`, `events.ndjson`,
`artifacts/` y `snapshots/`. Empaquetarlo en zip/zstd exige dependencias que 0.1 no necesita.

## Decisión

Contenedor propio y determinista:

```
HLIAR1\n
FILE <len> <path>\n<bytes>\n
...
```

Orden fijo: `manifest.json`, `state.json`, `events.ndjson`, `artifacts/<sha256>`.

El manifiesto incluye `files[{path,sha256,bytes}]`; `exported_at` queda fuera del hash del
manifiesto, que se calcula como SHA-256 de su JCS sin ese campo.

## Consecuencias

- Import verifica hashes de archivo, cadena de eventos y `chain_head` antes de escribir.
- 0.2 puede envolver el mismo contenido en zip/zstd sin cambiar el modelo.
- El nombre del archivo no forma parte del hash; el `manifest_hash` se publica en la respuesta.
