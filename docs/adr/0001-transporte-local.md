# ADR-0001 — Transporte local de 0.1: TCP + NDJSON

**Estado:** aceptada (0.1) · **Fecha:** 2026-11-07

## Contexto

La especificación describe una API local con HTTP/WebSocket (Apéndice B: Axum). Para 0.1 el
criterio de salida solo exige que un segundo proceso CLI recupere el estado tras reiniciar el
daemon. Añadir HTTP/WebSocket ahora introduce dependencias y superficie que 0.1 no necesita.

## Decisión

El daemon escucha en `127.0.0.1:<puerto efímero>` y habla JSON delimitado por línea. Cada
petición es `{"id","cmd","args"}`; cada respuesta `{"id","ok","data"|"error"}`.

El puerto y el pid se publican en `$HLI_HOME/daemon.json`.

## Consecuencias

- Cero dependencias de red en 0.1; el contrato de comandos no cambia.
- 0.2 sustituye el transporte por HTTP/WebSocket manteniendo `cmd`/`args`/`data`.
- El cliente no escribe SQLite: toda mutación pasa por el actor del daemon.
