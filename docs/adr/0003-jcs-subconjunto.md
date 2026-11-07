# ADR-0003 — JCS como subconjunto documentado

**Estado:** aceptada (0.1) · **Fecha:** 2026-11-07

## Decisión

`hli-core::canon` implementa RFC 8785 con estas reglas:

- Claves ordenadas por code units UTF-16.
- Sin espacios; UTF-8 sin BOM.
- Escapes mínimos (`\"`, `\\`, `\n`, `\r`, `\t`, `\b`, `\f` y `\u00xx` para el resto de controles).
- Números con la representación de `serde_json` (ryu para flotantes).

## Límites

- No se admite explícitamente el rango completo de JCS para números fuera del rango IEEE-754
  normal; `serde_json` rechaza NaN/Infinity, que es lo que exige la especificación.
- Las claves no ASCII se ordenan por UTF-16, como JCS; está cubierto por el comparador.

## Verificación

Pruebas unitarias en `crates/hli-core/src/canon.rs`: orden de claves, estabilidad anidada,
escapes y prefijo del hash.
