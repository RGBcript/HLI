# HLI — Recuperación de contexto (v2)

**Fecha:** 2026-11-07 · **Alcance:** artifact slices, payload verificable y política adaptativa.

## 1. Artifact slices

Un slice es una porción persistida de un artefacto, con hash propio y razón de inclusión.

```json
{
  "slice_id": "sl_...",
  "artifact_id": "art_...",
  "artifact_hash": "sha256:...",
  "media_type": "text/plain",
  "selector": {"kind": "line_range", "start": 120, "end": 220},
  "slice_hash": "sha256:...",
  "estimated_tokens": 840,
  "reason": "contains compiler failure"
}
```

Selectores soportados en esta fase:

| Selector | Comportamiento |
|---|---|
| `line_range` | líneas `start..end` (0-based, `end` exclusivo) |
| `byte_range` | bytes `start..end` |
| `full_text_if_small` | todo el artefacto si ≤ 4 MiB |

El contenido se guarda en `artifact_slices.content` (BLOB), con `slice_hash` verificable. No hay
chunking multimedia, OCR ni embeddings.

## 2. Payload canónico

El proveedor no recibe el prompt suelto: recibe el payload construido desde el manifiesto.

```json
{
  "instructions": [],
  "user_input": {"content": "...", "content_hash": "sha256:..."},
  "memories": [],
  "recent_events": [],
  "summaries": [],
  "artifact_slices": []
}
```

- `payload_hash = SHA-256(JCS(payload))`, persistido en el manifiesto antes de `model.requested`.
- Antes de llamar al proveedor, el daemon **reconstruye** el payload desde el manifiesto y los
  slices persistidos, y compara el hash. Si no coincide, el run falla con `conflict`.
- El hash cubre memorias, eventos, slices y prompt, no solo `user_input`.

## 3. Recuperación

Base léxica FTS5 con filtros estructurales antes del ranking:

- `workspace_id` (siempre);
- `status = active`;
- vigencia: `valid_from <= now` y (`valid_until` nulo o futuro);
- ámbito y tipo (previstos en el esquema; el filtro de tipo está disponible en la consulta);
- expansión léxica: términos del prompt y del contenido de memorias; nombres de archivo y
  símbolos entran como términos si aparecen en el texto.

Límite actual: top-5 por BM25, sin embeddings. La selección estructural de artefactos es explícita
(`--artifact` en el run) más una política por tipo de tarea.

## 4. Política adaptativa

| Tarea | Contexto |
|---|---|
| `memory_lookup` | hasta 3 memorias (400 chars cada una) + 2 eventos |
| `artifact_analysis` | slice del artefacto + 1 memoria; sin historial |
| `code_fix` | slice del archivo + 1 memoria; sin historial |
| `summarization` | slice contiguo que cubre el artefacto + 1 memoria |
| `integration` | slices + 2 memorias + 2 eventos |

La clasificación es heurística por palabras clave (`resume`, `log`, `corrige`, `integra`,
`archivo`). No usa un modelo.

## 5. Resultado medido

Con el benchmark v2 (3 repeticiones, 5 tareas):

- HLI v1 (sin slices): 9/15 de calidad, 6 reintentos, 159 tokens de entrada medios.
- HLI v2 (con slices): **15/15**, 0 reintentos, 983 tokens de entrada medios.
- Baseline: 15/15, 846 tokens, 0 reintentos.

Los slices son la diferencia entre fallar y acertar en `log` y `files`. El coste total de v2 es
un 9 % menor que el baseline en este conjunto, pero **no uniforme**: v2 es más caro en 3 de 5
tareas. No se puede afirmar un ahorro general.

## 6. Límites

- Sin embeddings; la recuperación semántica queda fuera.
- Sin chunking de binarios ni OCR.
- El selector de línea no ajusta por tokens: si un slice es enorme, entra completo.
- La política es heurística, no aprendida.
