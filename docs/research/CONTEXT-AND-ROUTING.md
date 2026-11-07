# HLI — Contexto adaptativo y enrutamiento

**Investigación previa a cambiar la política de contexto o routing.**
Fecha: 2026-11-07 · Estado: revisión bibliográfica, sin modificar HLI.

## 1. Adaptive-k (EMNLP 2025)

- **Título:** Efficient Context Selection for Long-Context QA: No Tuning, No Iteration, Just Adaptive-k
- **Autor:** Chihiro Taguchi · **Año:** 2025 · **URL:** https://arxiv.org/abs/2506.08479
- **Tipo:** **paper revisado por pares** (EMNLP 2025 Main) · **Código:** no verificado
- **Problema:** fijar `k` desperdicia tokens o omite evidencia; los métodos adaptativos con prompting iterativo (Self-RAG, Self-Route) funcionan en QA factoid pero fallan en agregación.
- **Método:** selección **single-pass** del número de pasajes a partir de la distribución de scores de similitud query-pasaje. Sin fine-tuning, sin inferencias LLM extra, sin cambiar el pipeline retriever-reader.
- **Datasets:** benchmarks de QA factoid y de agregación; cinco LCLMs y dos modelos de embeddings.
- **Resultado:** iguala o supera a `fixed-k` usando **hasta 10× menos tokens** que el contexto completo y recuperando **70 %** de los pasajes relevantes. Mejora la precisión en los cinco modelos.
- **Limitaciones:** depende de scores de similitud calibrados; no resuelve agregación con evidencia dispersa; no es determinista si el retriever no lo es.
- **Aplicabilidad a HLI:** HLI usa `top_k = 5` fijo y FTS5/BM25. La caída de scores BM25 es una señal más ruidosa que la coseno, pero la idea es aplicable: **k variable por caída entre scores consecutivos**, con un mínimo y un máximo.

## 2. Tail-Aware Adaptive-k (ECML PKDD 2026)

- **Título:** Tail-Aware Adaptive-k: Query-Adaptive Context Selection for RAG
- **Autores:** no listados en el abstract consultado · **Año:** 2026
- **URL:** https://arxiv.org/abs/2606.11907 · **Tipo:** **paper aceptado** (ECML PKDD 2026)
- **Método:** detección de rodilla en la curva de similitudes (patrón steep–flat–steep) para una región candidata compacta; validación de cola con teoría de valores extremos (EVT) localizada.
- **Complejidad:** de `O(N²M)` a `O(√(N log N)·M)`.
- **Datasets:** WebQuestions, 2WikiMultiHopQA, MuSiQue.
- **Resultado:** F1 **a 2–3 % del oráculo**, con órdenes de magnitud menos cómputo que EVT global; robusto a modelos de embeddings y dimensiones.
- **Limitaciones:** asume distribución de similitudes con cola; requiere embeddings; más complejo que Adaptive-k simple.
- **Aplicabilidad a HLI:** **test first**. Sin embeddings, la curva BM25 no cumple las assumptions; implementar la rodilla simple (knee) sin EVT es viable y barato.

## 3. Adaptive-RAG (NAACL 2024)

- **Título:** Adaptive-RAG: Learning to Adapt Retrieval-Augmented LLMs through Question Complexity
- **Autores:** Soyeong Jeong et al. · **Año:** 2024 · **URL:** https://arxiv.org/abs/2403.14403
- **Tipo:** **paper revisado por pares** (NAACL 2024) · **Código:** https://github.com/starsuzi/Adaptive-RAG
- **Método:** clasificador pequeño entrenado para predecir complejidad de la consulta y elegir entre **no-retrieval**, **single-step** e **iterative**.
- **Resultado:** mejora eficiencia y precisión frente a baselines adaptativos en QA abierto.
- **Limitaciones:** requiere entrenamiento y datos etiquetados; el clasificador puede equivocarse; pensado para QA, no para software.
- **Aplicabilidad a HLI:** la tríada **DIRECT / AUGMENT / RETRIEVE** es adoptable como **reglas**, sin entrenar: consulta de memoria → DIRECT (solo memoria); análisis de artefacto → RETRIEVE (slice); integración → AUGMENT (ambos). HLI ya tiene una clasificación heurística de tareas; falta mapearla a esas tres vías y medir.

## 4. LLMRouterBench (Findings ACL 2026)

- **Título:** LLMRouterBench: A Massive Benchmark and Unified Framework for LLM Routing
- **Autores:** Hao Li, Yiqun Zhang, Zhaoyan Guo et al. · **Año:** 2026
- **URL:** https://arxiv.org/abs/2601.07206 · **Tipo:** **Findings ACL 2026** · **Código:** https://github.com/ynulihao/LLMRouterBench
- **Escala:** >400K instancias, 21 datasets, 33 modelos, 10 routers representativos.
- **Hallazgos:**
  - Confirma complementariedad entre modelos (premisa del routing).
  - **Muchos métodos de routing rinden igual bajo evaluación unificada.**
  - **Varios enfoques recientes, incluidos routers comerciales, no superan de forma fiable un baseline simple.**
  - Persiste una brecha grande frente al oráculo, causada sobre todo por **fallos de model recall** (el router no elige el modelo que sí sabía resolver).
  - Los embeddings del backbone tienen impacto limitado; ensambles más grandes muestran rendimientos decrecientes frente a una curación cuidadosa.
  - Permite análisis con latencia.
- **Limitaciones:** benchmarks y modelos de texto; no evalúa routing de herramientas ni de memoria.
- **Aplicabilidad a HLI:** **no enrutar sin perfil de capacidad y sin medir model recall**. Un router que elige un secundario barato pero incapaz produce el 0/15 que ya observamos. La política debe poder emitir `route = primary`, `reason = no_eligible_secondary` sin intentar el secundario.

## 5. Matriz de métodos de contexto

| Método | Reduce tokens | Calidad | Determinista | Embeddings | Coste extra | Complejidad | Evidencia | Veredicto HLI |
|---|---|---|---|---|---|---|---|---|
| `fixed top-k` | no | media | sí | no | 0 | baja | baseline | actual |
| `Adaptive-k` (caída de scores) | **sí, hasta 10×** | igual o mejor | sí (con BM25) | no | 0 | baja | EMNLP 2025 | **adoptar** |
| `score thresholding` | sí | variable | sí | no | 0 | baja | común | adaptar (junto a k) |
| `retrieval confidence` | sí | variable | depende | no | 0 | media | común | test first |
| `DIRECT/AUGMENT/RETRIEVE` | sí | mejor si clasifica bien | sí (reglas) | no | 0 | media | NAACL 2024 | **adaptar** |
| `long-context input` | no | alta | sí | no | alto | baja | baseline caro | rechazar como política |
| `lexical (FTS5/BM25)` | — | media | sí | no | 0 | baja | HLI | mantener |
| `dense retrieval` | — | alta | no | sí | embedding | media | común | test first (no ahora) |
| `hybrid` | — | alta | no | sí | embedding | media | común | rechazar por ahora |
| `reranking` | no (añade) | alta | no | sí | LLM/reranker | alta | común | rechazar por coste |

## 6. Decisión propuesta para HLI

1. **Sustituir `top_k = 5` fijo por k variable** basado en:
   - caída relativa entre scores consecutivos (ratio `s[i]/s[i+1]`),
   - mínimo 1 y máximo configurable (p. ej. 8),
   - corte si el score absoluto es bajo,
   - presupuesto disponible.
   Sin embeddings; con BM25.
2. **Mapear la clasificación de tarea a tres vías**:
   - `memory_lookup` → DIRECT (memoria, sin eventos),
   - `artifact_analysis`/`code_fix`/`summarization` → RETRIEVE (slice),
   - `integration` → AUGMENT (memoria + slices).
   Ya existe la clasificación; falta la vía explícita y medirla.
3. **Routing**:
   - Solo si hay un secundario con **perfil de capacidad** medido (no por precio).
   - Si no cumple, `route = primary`, `reason = no_eligible_secondary`.
   - Métrica principal: **coste por tarea exitosa**, no coste por token.
   - Registrar model recall: ¿el router eligió el modelo que sabía resolver?

## 7. Respuestas con evidencia

- **Cómo determinar k:** caída de scores + presupuesto (Adaptive-k), no un fijo.
- **Cuándo usar DIRECT:** cuando la respuesta está en memoria con score alto y no se necesita evidencia externa.
- **Cuándo no enrutar:** cuando no hay secundario con capacidad demostrada; el coste barato no compensa el 0/15.
- **Qué no se adopta:** embeddings, reranking y routers entrenados; no hay evidencia local que los justifique y añaden coste.
