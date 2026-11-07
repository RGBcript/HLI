# HLI — Memoria, conflicto y procedencia

**Investigación previa a cualquier cambio en el Context Engine.**
Fecha: 2026-11-07 · Estado: revisión bibliográfica, sin modificar HLI.

## 1. MemConflict (2026)

- **Título:** MemConflict: Evaluating Long-Term Memory Systems Under Memory Conflicts
- **Autores:** Zhen Tao, Jinxiang Zhao, Peng Liu, Dinghao Xi, Yanfang Chen, Wei Xu, Zhiyu Li
- **Año:** 2026 · **URL:** https://arxiv.org/abs/2605.20926
- **Tipo:** preprint (arXiv, cs.IR) · **Código:** https://github.com/TaoZhen1110/MemConflict · **Licencia:** no verificada
- **Problema:** las evaluaciones de memoria miden la respuesta final o la actualización temporal, pero no cómo se recupera y ordena evidencia válida, correcta y aplicable bajo alternativas en conflicto.
- **Método:** marco diagnóstico que formaliza conflictos **dinámicos**, **estáticos** y **condicionales** sobre validez temporal, corrección factual y aplicabilidad contextual. Genera historias largas controladas desde perfiles estructurados, inyecta conflictos entre sesiones y distractores semánticamente similares.
- **Datasets:** diálogos multi-sesión sintéticos derivados de perfiles.
- **Sistemas evaluados:** seis sistemas representativos de memoria a largo plazo.
- **Métricas:** corrección de respuesta, recuperación de memoria de soporte, ranking; análisis de sensibilidad a longitud, distractores, consultas implícitas y distancia de conflicto.
- **Resultado:** fortalezas desiguales por tipo de conflicto; **la corrección de la respuesta a menudo diverge de la recuperación y el ranking**. Más historia, más distractores y consultas implícitas degradan.
- **Limitaciones:** datos sintéticos; no es un benchmark de producción; no publica una tabla única de cifras en el abstract.
- **Aplicabilidad a HLI:** HLI mide respuesta final en v1–v3. MemConflict justifica separar **estado de memoria**, **retrieval**, **ranking** y **generación** — exactamente lo que pide el protocolo experimental.

## 2. MemStrata / validez temporal (2026)

- **Título:** Temporal Validity in Retrieval Memory: Eliminating Stale-Fact Errors for AI Agents over Evolving Knowledge
- **Autores:** Neeraj Yadav et al. · **Año:** 2026
- **URL:** https://arxiv.org/abs/2606.26511 · **Tipo:** preprint (cs.CL)
- **Problema:** RAG no tiene modelo del tiempo. Cuando un hecho cambia, recupera el valor obsoleto y el actual con similitud casi idéntica.
- **Dato clave:** en un dataset calibrado, la similitud coseno distingue un hecho contradicho de uno duplicado con **AUROC 0.59** (casi azar). Las contradicciones son a menudo **más** similares al original que las paráfrasis.
- **Método:** ledger **bi-temporal**; cuando el valor de un hecho se contradice, una regla determinista **(subject, relation, object)** retira el valor obsoleto. Sin umbral de similitud y **sin llamada LLM**.
- **Datasets:** seis benchmarks, ejecución local con modelo 7B.
- **Resultado:** empata con RAG en conocimiento estático; **0.95–1.00** en conocimiento evolutivo frente a **0.20–0.47** de RAG. Tasa de error por hecho obsoleto: RAG **15–40 %**; MemStrata **~0 %**. Latencia de recuperación **~2,1 s** frente a **16–18 s** de reranking con LLM.
- **Limitaciones:** SRO funciona para hechos; no está claro para decisiones de software, restricciones procedurales o resúmenes. Requiere extracción fiable de SRO.
- **Código/datos:** el abstract anuncia harness, datasets y protocolo; licencia no verificada.
- **Companion:** https://arxiv.org/abs/2608.20685 valida el enfoque en historiales reales de GitHub (SWE-bench buggy→fixed).
- **Aplicabilidad a HLI:** HLI ya guarda `valid_from`, `valid_until`, `supersedes`, `contradicts`. Falta `superseded_at` y una regla determinista de retirada para hechos. **No** adoptar SRO obligatorio para decisiones.

## 3. LatticeMind (2026)

- **Título:** LatticeMind: A Conflict-Aware Memory Primitive for Multi-Agent Systems
- **Autores:** Heng Zhou et al. · **Año:** 2026 · **URL:** https://arxiv.org/abs/2608.08236
- **Tipo:** preprint (cs.AI) · **Código/licencia:** no verificados
- **Problema:** los sistemas multiagente no tienen un mecanismo persistente para decidir qué afirmación incompatible se debe creer.
- **Método:** memoria estructurada con **estado explícito por ítem**, comprobaciones simbólicas de conflicto baratas y **reconciliación LLM solo para casos semánticos no resueltos**.
- **Evaluación:** ConflictBank con **label-blind** (sin pistas del nombre de la fuente).
- **Resultado:** **0,97** de precisión frente a **0,61** del mejor baseline de agregación; McNemar p<10⁻⁶. Quitar el checker o el reconciler cuesta **12–14 puntos**. En cuatro benchmarks secundarios de planificación gana a merge naive en 3/4, pero no sustituye a deliberación.
- **Limitaciones:** evalúa conflictos de afirmaciones, no memoria temporal de software; el reconciliador LLM añade coste y no es determinista.
- **Aplicabilidad a HLI:** HLI tiene `contradicts` como lista, pero no un **conjunto de conflicto con estado** ni una resolución registrada. Adoptar el estado explícito y el checker simbólico; el reconciliador LLM queda como `test first` por coste.

## 4. FEB-30 (2026)

- **Título:** Beyond Downstream QA: Rethinking How We Evaluate Agent Memory
- **Autores:** Sebastian-Antonio Toma et al. · **Año:** 2026 · **URL:** https://arxiv.org/abs/2610.22209
- **Tipo:** preprint (cs.CL) · **Código:** https://github.com/amazon-science/feb-30-benchmark
- **Problema:** medir solo QA final confunde corrección de memoria con retrieval y generación.
- **Método:** 30 trayectorias de evolución de hechos, 6 categorías y 3 dificultades, generadas por inversión de paradigma (documentos desde trayectorias conocidas), con verdad determinista sin anotación humana.
- **Resultado:** evaluación intrínseca y extrínseca divergen **16 puntos porcentuales** de media; **44 %** de trayectorias superan el umbral de divergencia. El sistema evaluado clasifica correctamente las operaciones de memoria (create, update, reinforce, supersede) solo **20 %** de las veces — un fallo invisible a la evaluación extrínseca.
- **Limitaciones:** un solo sistema evaluado; trayectorias sintéticas; no cubre software.
- **Aplicabilidad a HLI:** es la justificación directa de separar **estado interno** de **respuesta final**. HLI debe registrar si una memoria se creó, reforzó, actualizó o supersedió, y evaluarlo aparte.

## 5. Evidence tracing y provenance (2026)

- **Título:** From Agent Traces to Trust: A Survey of Evidence Tracing and Execution Provenance in LLM Agents
- **Autores:** no listados en el abstract consultado · **Año:** 2026
- **URL:** https://arxiv.org/abs/2606.04990 · **Tipo:** survey (arXiv)
- **Problema:** una respuesta correcta no revela qué evidencia la sostuvo, si las tool calls se justificaron, cómo la memoria influyó ni dónde falló la ejecución.
- **Método:** marco con **execution provenance** (grafo tipado de la ejecución) y **evidence tracing** (proyección sobre relaciones de soporte). Taxonomía de seis dimensiones: fuentes de traza, unidades de evidencia/ejecución, relaciones de procedencia, granularidad y momento, formas de representación y funciones de confianza. Nueve relaciones de procedencia y dos atributos transversales (origen y assurance).
- **Hallazgos:** poca integración de procedencia fina con múltiples funciones de confianza; **ningún benchmark del catálogo evalúa la cadena completa de procedencia**; poca evidencia de trazas a prueba de manipulación.
- **Limitaciones:** survey, no resultados experimentales propios.
- **Aplicabilidad a HLI:** HLI ya tiene receipts, `ContextManifest` y `payload_hash`, pero no registra **qué memoria o slice sostuvo qué parte de la respuesta**. Añadir `used_memory_ids` y `used_slice_ids` al receipt es barato y cierra parte del gap.

## 6. Matriz comparativa

| Método | Temporalidad | Contradicciones | Distractores | Evidencia/procedencia | Coste | Llamadas LLM extra | Embeddings | Determinista | Código | Compatibilidad HLI |
|---|---|---|---|---|---|---|---|---|---|---|
| MemConflict (marco) | sí (validez) | dinámicas, estáticas, condicionales | sí (similares) | recuperación + ranking | bajo (eval) | 0 | no | sí | sí | alta (protocolo) |
| MemStrata | bi-temporal | supersesión SRO | no evalúa | ledger | bajo | 0 | no | sí | anunciado | alta para hechos; media para decisiones |
| LatticeMind | estado por ítem | checker + reconciler | parcial | estado de conflicto | medio | sí (casos no resueltos) | no | parcial | no verificado | media |
| FEB-30 | trayectorias | create/update/reinforce/supersede | no | intrínseca + extrínseca | bajo | 0 | no | sí | sí | alta (evaluación) |
| Evidence tracing | no | no | no | grafo de procedencia | bajo | 0 | no | sí | no aplica | alta (receipt) |
| HLI actual | `valid_from/until` | lista `contradicts` | FTS5 | receipt + manifiesto | bajo | 0 | no | sí | propio | — |

## 7. Campos de memoria: qué mantener

| Campo | ¿Mantener? | Motivo |
|---|---|---|
| `observed_at` | **sí** | cuándo se observó, distinto de cuándo empieza a ser válido |
| `valid_from` | **sí** | ya existe; base de la vigencia |
| `valid_until` | **sí** | ya existe; permite caducidad sin borrar |
| `superseded_at` | **sí** | hoy solo hay `status`; falta el instante de retirada |
| `subject`, `relation`, `object` | **opcional** | útil para hechos; **no** obligatorio para decisiones, restricciones y resúmenes |
| `source authority` | **sí** | quién lo afirma y con qué autoridad; hoy `authorship` es genérico |
| `contradiction set` | **sí** | lista de memorias en conflicto, con estado |
| `applicability condition` | **test first** | coste de expresar condiciones; validar primero con casos reales |

**Decisión:** adoptar `superseded_at`, `source_authority` y `contradiction_set`; mantener SRO como campos **opcionales** para memorias factuales. No imponer SRO a decisiones de software.

## 8. Respuestas con evidencia

- **Hechos obsoletos:** supersesión determinista con ledger bi-temporal (MemStrata), no umbral de similitud.
- **Contradicciones:** estado explícito + checker simbólico (LatticeMind); reconciliación LLM solo si el checker no resuelve, y con coste contado.
- **Procedencia:** registrar `used_memory_ids` y `used_slice_ids` en el receipt (evidence tracing).
- **Qué no se adopta todavía:** SRO obligatorio, embeddings, reconciliador LLM en el write path.
