# HLI — Experimento de supersesión temporal

**Fecha:** 2026-11-07 · **Alcance:** supersesión explícita determinista y procedencia en el receipt.
**Crudo:** `docs/research/TEMPORAL-SUPERSESSION-RAW.jsonl`

## 1. Cambios aplicados

- `memories` añade `observed_at`, `superseded_at`, `source_authority`, `subject`, `relation`,
  `object` (los tres últimos opcionales, solo para hechos).
- `memory.commit` acepta `supersedes = [id]`: en la **misma transacción** inserta la nueva y marca
  la anterior `superseded`, `superseded_at = now`, `valid_until = now`.
- No hay inferencia por BM25 ni por LLM.
- `search` sigue filtrando `status = 'active'` y vigencia; `get` permite leer la histórica.
- El receipt añade `used_memory_ids`, `used_slice_ids`, `used_event_ids`,
  `candidate_memory_ids`, `rejected_memory_ids`, `rejection_reasons`.

## 2. Pruebas

`cargo test --test e2e_07`: **7/7**.

| Prueba | Resultado |
|---|---|
| `explicit_supersession_retires_old_memory` | ok |
| `superseded_memory_is_not_retrieved` | ok |
| `historical_query_can_read_superseded_memory` | ok |
| `supersession_is_atomic` | ok (id inexistente → error, nada insertado) |
| `decision_memory_does_not_require_sro` | ok |
| `receipt_records_used_memory_and_slice_and_excludes_candidates` | ok |
| `receipt_survives_restart_and_export_import` | ok |

## 3. Experimento

- **20 pares** old → current (límites de recursos), **10 memorias sin reemplazo** (puertos) y
  **10 distractores léxicos**.
- **A:** sin supersesión; old y current quedan activas.
- **B:** current se registra con `--supersedes old`.
- 1 repetición por consulta y brazo; 30 consultas por brazo.
- Modelo `gemma4:31b-cloud`, temperatura 0, `top_k = 5`.
- Workspace distinto por brazo; sin respuestas del benchmark en la memoria.

## 4. Resultados

| Grupo | Brazo | n | Stale retrieval | Current recall | Ranking ok | Calidad | Tokens |
|---|---|---|---|---|---|---|---|
| Pares | A | 20 | **75 %** | 75 % | 0 % | 11/20 | 219,1 |
| Pares | B | 20 | **0 %** | **100 %** | **100 %** | **13/20** | 218,5 |
| Sueltas | A | 10 | 0 % | 80 % | 80 % | 1/10 | 290,1 |
| Sueltas | B | 10 | 0 % | 80 % | 80 % | 1/10 | 280,4 |

Estado interno en B: **20/20** memorias antiguas con `status = superseded` y `superseded_at`
no nulo.

## 5. Criterios

| Criterio | Resultado | ¿Cumple? |
|---|---|---|
| `stale-memory retrieval rate = 0 %` | B: 0 % (A: 75 %) | **sí** |
| `current-memory recall = 100 %` | B: 100 % (A: 75 %) | **sí** |
| Calidad no inferior | B 13/20 vs A 11/20; sueltas 1/10 vs 1/10 | **sí** |
| Sin llamadas LLM adicionales | 0 | **sí** |

## 6. Veredicto

**La supersesión explícita determinista alcanza los criterios y se adopta.** Elimina la
recuperación de la memoria obsoleta (75 % → 0 %), recupera siempre la vigente (75 % → 100 %) y
mejora el ranking y la calidad sin coste adicional.

## 7. Lo que el experimento no demuestra

- **La calidad absoluta es baja**: 13/20 en pares y 1/10 en sueltas. La supersesión arregla el
  estado y el retrieval, no la generación. Las sueltas fallan por el modelo/prompt, no por la
  memoria: el recall es 80 % y la respuesta no contiene el puerto esperado.
- **Una sola repetición**, corpus sintético, un modelo. No es un resultado de producción.
- **No se prueba `contradiction_set`**, ni reconciliación LLM, ni embeddings, ni routing.

## 8. Siguiente paso

Detenerse aquí. La supersesión está adoptada y probada; las contradicciones no se implementan en
este bloque.

## 9. Limitaciones de la procedencia

`used_memory_ids` corresponde a las memorias del manifiesto (las que se enviaron al proveedor),
no a todos los candidatos. `candidate_memory_ids` recoge los recuperados antes del truncado por
política; `rejected_memory_ids` son los candidatos que no entraron, con razón `policy`.
No se añadió una segunda búsqueda: los candidatos salen de la misma consulta que alimenta el
manifiesto.
