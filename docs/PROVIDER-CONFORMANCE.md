# HLI — Conformidad de proveedores

**Fecha:** 2026-11-07 · **Alcance:** proveedores reales y trazabilidad de usage/coste.

## Proveedores

| Proveedor | Implementación | Endpoint | Usage | Secretos | Estado |
|---|---|---|---|---|---|
| `sim` | `hli-agent::SimProvider` | ninguno | estimado | no | implementado |
| `ollama` | `hli-provider::OllamaProvider` | `POST /api/chat` | reportado (`prompt_eval_count`, `eval_count`) | no | **probado contra servidor real** |
| `openai-compatible` | `hli-provider::OpenAiProvider` | `POST /chat/completions` | reportado (`usage`) | sí, por handle | implementado; no probado con clave real |

`gemma4:31b-cloud` vía Ollama respondió con `usage_source: reported` y latencia real.

## Contrato de usage

```json
{
  "input_tokens": 37,
  "output_tokens": 3,
  "cached_tokens": 0,
  "latency_first_token_ms": 486,
  "latency_total_ms": 486,
  "finish_reason": "stop",
  "raw_usage_available": true,
  "usage_source": "reported"
}
```

- Si el proveedor no informa tokens: `usage_source: "estimated"` y `raw_usage_available: false`.
- Sin streaming en esta fase: `latency_first_token_ms` es igual al total y así se declara.
- El modelo reportado por el servidor se conserva (`gemma4:31b`, aunque se pidiera `gemma4:31b-cloud`).

## Trazabilidad

Cada llamada real queda ligada a:

- `Run` y `ContextManifest` (`receipt.context_manifest`);
- `model.requested` con `manifest_id`, proveedor y modelo;
- `model.responded` con usage y coste;
- `Reproducibility Receipt` con proveedor, modelo, adaptador, usage, coste y `result_hash`;
- `provider.handoff` cuando cambia el proveedor del thread.

**Garantía de payload:** antes de llamar al proveedor, el daemon verifica
`hash(JCS(user_input)) == manifest.user_input.content_hash`. Si no coincide, el run falla con
`conflict`. El proveedor no puede recibir contenido distinto del manifiesto.

## Secretos

- El valor se lee de `HLI_SECRET_<HANDLE>` o de `$HLI_HOME/secrets/<handle>`.
- Nunca se escribe en eventos, receipts, manifiestos ni SQLite.
- El receipt guarda `secret_handles: [{id, version}]`, donde `version` es un hash corto.

## Cambio de proveedor

`provider.handoff` conserva `from`, `to` y `model`. El workspace, la rama y el event log no
cambian; las credenciales son por llamada, no por sesión.

## Pendiente

- Streaming real (primer token medido, no estimado).
- `openai-compatible` contra un endpoint con clave.
- Coste con precios reales del proveedor (hoy es configuración del usuario).
- `cache` real: se reporta si el proveedor lo da, no se infiere.

## Actualización v2 (2026-11-07)

- El proveedor recibe un **payload canónico** (`instructions`, `user_input`, `memories`,
  `recent_events`, `summaries`, `artifact_slices`), no el prompt suelto.
- `payload_hash = SHA-256(JCS(payload))` se persiste en el manifiesto antes de `model.requested`.
- Antes de llamar, el daemon reconstruye el payload desde el manifiesto y los slices persistidos;
  si el hash no coincide, el run falla.
- `model.requested` incluye `payload_hash`.
- Los slices entran con `slice_hash` y `estimated_tokens`; su contenido se verifica al recargar.
