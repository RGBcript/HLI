# ADR-0006 — Run, ContextManifest y agent loop de 0.2

**Estado:** aceptada · **Fecha:** 2026-11-07

## Contexto

0.2 introduce ejecución: un modelo (simulado) recibe contexto y produce una respuesta.
La especificación exige que el `ContextManifest` sea inmutable y se persista **antes** de
`model.requested`, y que el run quede fijado a una revisión de rama.

## Decisión

1. `Run` fija `workspace_id`, `thread_id`, `branch_id` y `branch_revision` al crearse.
   `branch_revision` es `branches.head_sequence` en ese instante.
2. El manifiesto se construye en `hli-agent::build_manifest` con los eventos del thread cuya
   `sequence <= branch_revision`, recortados al presupuesto (8 192 tokens por defecto,
   `chars/4`); lo excluido se registra con `reason: budget`.
3. `save_manifest` sella el manifiesto (`content_hash` sobre el sobre sin el hash) y emite
   `context.manifest.created` **antes** de que el daemon emita `model.requested`.
4. El proveedor es `SimProvider`: determinista, sin red, sin coste. Deriva el texto del hash
   de `(run_id, prompt, manifest_hash)`.
5. El receipt se genera al terminar el run y se guarda en `runs.receipt_json`; incluye
   proveedor, modelo, manifiesto, permisos efectivos, sandbox declarado (`none`) y sampling.
6. Permisos: `filesystem.read` allow, `filesystem.write` ask, `shell.execute` deny. Un `ask`
   deja el run en `waiting_approval`; `run.approve` ejecuta y completa.

## Consecuencias

- El gate de 0.2 se prueba con `run_manifest_receipt_and_restart` y tres pruebas de permisos.
- El Tool Runtime está confinado al `root_path` del workspace; una ruta fuera se rechaza.
- No hay sandbox de sistema operativo en 0.2: `shell.execute` está denegado y el receipt lo
  declara explícitamente.
- El manifiesto se guarda completo (con `content_hash`) para poder reconstruirlo; el hash se
  calcula sobre el sobre sin ese campo.
