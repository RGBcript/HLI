# HLI — Síntesis de investigación y protocolo experimental

**Fecha:** 2026-11-07 · **Estado:** propuesta, sin modificar el runtime.
**Base:** `MEMORY-AND-PROVENANCE.md` y `CONTEXT-AND-ROUTING.md`.

## 1. Clasificación de mecanismos

| Mecanismo | Fuente | Clasificación | Por qué |
|---|---|---|---|
| Supersesión determinista bi-temporal | MemStrata (2606.26511) | **adopt** | stale 15–40 % → ~0 %, sin LLM, latencia 2,1 s |
| `superseded_at` y estado de retirada | MemStrata / FEB-30 | **adopt** | HLI solo tiene `status`; falta el instante |
| `contradiction_set` con estado | LatticeMind (2608.08236) | **adapt** | 0,97 vs 0,61, pero sin reconciliador LLM en write path |
| `source_authority` | LatticeMind / provenance | **adapt** | útil; HLI tiene `authorship` genérico |
| `used_memory_ids` / `used_slice_ids` en receipt | Evidence tracing (2606.04990) | **adopt** | cierra el gap de atribución; coste casi cero |
| k variable por caída de scores | Adaptive-k (2506.08479) | **adapt** | 10× menos tokens; con BM25, no coseno |
| Rodilla + EVT | TAA-k (2606.11907) | **test first** | asume embeddings; sin ellos, solo la rodilla |
| DIRECT/AUGMENT/RETRIEVE | Adaptive-RAG (2403.14403) | **adapt** | reglas, sin clasificador entrenado |
| Routing condicionado a capacidad | LLMRouterBench (2601.07206) | **adopt como regla** | routers simples no superan baseline; model recall |
| SRO obligatorio | MemStrata | **reject** | no cubre decisiones, restricciones ni resúmenes |
| Reconciliador LLM en write | LatticeMind | **test first** | +12–14 pts, pero coste y no determinismo |
| Embeddings / dense retrieval | literatura RAG | **insufficient evidence** | no hay medición local que lo justifique |
| Reranking | literatura RAG | **reject (por coste)** | 16–18 s frente a 2,1 s en MemStrata |
| Long-context como política | baseline v3 | **reject** | 68 540 tokens en log_big |

## 2. Adopciones propuestas

### A1. Retirada determinista con `superseded_at`

- **Evidencia:** MemStrata, stale-fact error 15–40 % → ~0 %; AUROC de similitud 0,59.
- **Modificación:** añadir `superseded_at` a `memories`; al registrar una memoria que supersede a otra, marcar la anterior `status=superseded`, `superseded_at=now`, `valid_until=now`. Sin SRO para decisiones: la relación se declara en `supersedes`.
- **Coste:** baja (una columna, un UPDATE en la misma transacción).
- **Riesgo:** supersesión errónea si el usuario no declara `supersedes`.
- **Prueba falsable:** con 20 pares stale/current, la tasa de recuperación de la obsoleta debe ser 0.
- **Rollback:** quitar el UPDATE y volver a `status` manual.

### A2. `contradiction_set` con estado

- **Evidencia:** LatticeMind, 0,97 vs 0,61; ablación −12/−14 pts sin checker.
- **Modificación:** tabla `memory_conflicts(id, memory_a, memory_b, state, detected_at, resolved_by)`; estados `open`, `resolved_a`, `resolved_b`, `both_valid_conditional`. Detección simbólica: misma `subject`+`relation`, distinto `object`, sin `valid_until` que las separe.
- **Coste:** media (tabla + chequeo en commit).
- **Riesgo:** falsos positivos en memorias narrativas.
- **Prueba falsable:** 15 conflictos etiquetados; el checker debe detectar ≥12 y no marcar más de 2 falsos.
- **Rollback:** desactivar el chequeo; `contradicts` sigue como lista manual.

### A3. Atribución en el receipt

- **Evidencia:** survey de provenance; gap de atribución memoria→respuesta.
- **Modificación:** `receipt.used_memory_ids`, `receipt.used_slice_ids`, `receipt.used_event_ids`.
- **Coste:** muy baja.
- **Riesgo:** ninguno funcional; el receipt crece.
- **Prueba falsable:** en una respuesta que cita una decisión, el id de esa memoria debe estar en `used_memory_ids`.
- **Rollback:** quitar los campos.

### A4. k variable por caída de scores

- **Evidencia:** Adaptive-k, hasta 10× menos tokens, 70 % de pasajes relevantes.
- **Modificación:** en `search_memories`, calcular `k` como el punto donde `score[i]/score[i+1]` cae por debajo de un umbral (p. ej. 1,25), con mínimo 1 y máximo 8; si el mejor score es bajo, `k=0`.
- **Coste:** baja (una función, sin esquema).
- **Riesgo:** BM25 no está calibrado como coseno; el umbral debe medirse.
- **Prueba falsable:** en las 12 tareas del protocolo, tokens −30 % y precisión no inferior.
- **Rollback:** volver a `k=5` con una constante.

### A5. Tres vías explícitas

- **Evidencia:** Adaptive-RAG; HLI ya clasifica tareas.
- **Modificación:** `context_route` en el manifiesto: `direct` (memoria), `retrieve` (slice), `augment` (ambos). La política actual se mapea a estas tres.
- **Coste:** baja.
- **Riesgo:** clasificación heurística errónea.
- **Prueba falsable:** en 12 tareas, la vía elegida debe coincidir con la evidencia necesaria en ≥10.
- **Rollback:** ignorar `context_route` y usar la política actual.

### A6. Routing solo con capacidad demostrada

- **Evidencia:** LLMRouterBench; el 0/15 local con qwen1.5B.
- **Modificación:** `run.start` acepta `secondary_model` solo si existe un `capability_profile` con ≥N muestras y model recall medido; si no, `route=primary`, `reason=no_eligible_secondary`.
- **Coste:** media (perfil + registro).
- **Riesgo:** perfil sobreajustado a pocas tareas.
- **Prueba falsable:** en 20 tareas, el routed no debe perder más de 1 punto de calidad frente a primary.
- **Rollback:** desactivar routing y dejar `primary`.

## 3. Protocolo experimental

**Independiente de v1–v3.** 12 casos, 5 repeticiones, temperatura 0.

| Caso | Qué prueba |
|---|---|
| `cold_memory` | no hay memoria; debe decir "no consta" |
| `correct_warm_memory` | memoria vigente correcta |
| `stale_memory` | memoria obsoleta no debe recuperarse |
| `dynamic_conflict` | dos valores, uno posterior válido |
| `static_conflict` | dos valores incompatibles sin fecha |
| `conditional_conflict` | ambos válidos bajo condiciones distintas |
| `semantically_similar_distractor` | distractor léxicamente cercano |
| `out_of_order_ingestion` | eventos ingeridos fuera de orden temporal |
| `compact_direct_input` | respuesta cabe en el prompt, sin retrieval |
| `large_localized_artifact` | slice de una zona de un artefacto grande |
| `aggregate_evidence` | evidencia repartida en 3 artefactos |
| `missing_evidence` | no hay evidencia; no debe inventar |

### Evaluación en cuatro capas

1. **Estado interno:** ¿la memoria se creó, reforzó, actualizó o supersedió correctamente? (FEB-30: solo 20 % en el sistema evaluado).
2. **Retrieval:** ¿se recuperó la evidencia correcta? (recall@k).
3. **Ranking:** ¿la evidencia vigente quedó por delante de la obsoleta/contradictoria?
4. **Generación:** ¿la respuesta usó la evidencia recuperada? (cita el id).
5. **Resultado final:** corrección de la respuesta.

No se acepta precisión final como única métrica: una respuesta correcta puede ocultar memoria rota (FEB-30, divergencia 16 pp).

## 4. Experimento mínimo propuesto

**Hipótesis falsable:** sustituir `top_k=5` fijo por k variable por caída de scores reduce los tokens de entrada ≥30 % sin reducir la corrección final en las 12 tareas.

- **Único cambio:** `search_memories` (k variable). No se toca el esquema, ni el manifiesto, ni los slices, ni el proveedor.
- **Brazos:** A = top-5 actual; B = k variable (umbral 1,25; min 1; max 8).
- **Ejecución:** 12 tareas × 2 brazos × 5 reps = 120 llamadas con `gemma4:31b-cloud`, temperatura 0.
- **Métricas:** tokens de entrada, coste, latencia, corrección final, recall de memoria, k medio.
- **Criterio de éxito:** tokens −30 %, corrección no inferior (≤1 caso de diferencia), latencia no peor.
- **Criterio de rollback:** si la corrección cae en ≥2 casos, se revierte a top-5.
- **Coste:** ~2 h de ejecución local, 0 API.
- **Qué no prueba:** supersesión, conflictos ni routing. Es un experimento, no la fase entera.

## 5. Respuestas al gate

| Pregunta | Respuesta con evidencia |
|---|---|
| ¿Qué método para hechos obsoletos? | Supersesión determinista bi-temporal (MemStrata). |
| ¿Qué método para contradicciones? | Estado explícito + checker simbólico (LatticeMind); LLM solo si no resuelve. |
| ¿Cómo elegir memoria o artifact slice? | Tres vías: DIRECT (memoria), RETRIEVE (slice), AUGMENT (ambos). |
| ¿Cómo determinar k dinámicamente? | Caída de scores + presupuesto (Adaptive-k); validar con BM25. |
| ¿Cuándo usar DIRECT? | Memoria con score alto y respuesta contenida en ella. |
| ¿Cuándo no enrutar? | Sin perfil de capacidad/model recall; `no_eligible_secondary`. |
| ¿Cómo evaluar memoria sin confundirla con generación? | Cuatro capas separadas; FEB-30 y MemConflict. |
| ¿Qué componentes tienen código reutilizable? | FEB-30 y LLMRouterBench con repo; MemStrata anuncia harness; LatticeMind no verificado. Licencias sin comprobar. |

## 6. Lo que no se hace en esta etapa

- No se modifica HLI Core, ranking, esquema de memoria ni FTS5.
- No se añaden embeddings ni se entrena un router.
- No se declara que un método resolverá los problemas: solo se propone el experimento.
