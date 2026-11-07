# ADR-0002 — El sobre canónico incluye `previous_hash`

**Estado:** aceptada (corrige la especificación) · **Fecha:** 2026-11-07

## Contexto

La especificación (§15.8, punto 1) define el sobre canónico como «el objeto del evento sin el
campo `integrity`». Pero `previous_hash` vive dentro de `integrity`. Si se excluye, el hash del
evento no cubre el enlace con el anterior: se podría alterar `previous_hash` sin romper
`event_hash`, y la cadena dejaría de ser verificable.

## Decisión

El sobre canónico es el evento completo **sin `event_hash`**, e incluye:

```json
"integrity": { "canonicalization": "...", "hash_alg": "...", "previous_hash": "..." }
```

`event_hash = "sha256:" + hex(SHA-256(JCS(sobre)))`.

## Consecuencias

- La cadena cubre el enlace: alterar `previous_hash` cambia `event_hash`.
- Se corrige §15.8 de la especificación; la regla pasa a ser normative.
- Los eventos ya exportados con la regla anterior no serían verificables con esta; no hay
  producción previa, así que no hay migración.
