# HLI — Experimento Adaptive-k sobre FTS5

**Fecha:** 2026-11-07 · **Runtime:** sin cambios de esquema; `top_k` es un parámetro opcional con default 5.
**Dataset congelado:** `docs/research/adaptive-k-dataset.json`
**Hash del dataset:** `sha256:912fdc3b8370932dd64681f55b2c39119fd8a71e7b7c9c31b377cec014f09ab3`
**Crudo:** `docs/research/ADAPTIVE-K-RAW.jsonl`

## 1. Corrección de la adaptación a FTS5

`bm25()` en SQLite FTS5 devuelve **distancias negativas**: más negativo es mejor. El daemon ya
expone `abs(bm25)`, que en ese régimen es una **relevancia positiva** donde mayor es mejor.
El experimento usa esa transformación y **no** divide scores negativos.

Gap normalizado usado:

```
rel_i = abs(bm25_i)
gap_i = (rel_i - rel_{i+1}) / max(rel_i, 1e-9)
k = argmax(gap_i) + 1, si gap_i >= umbral
k = 5 (fallback) si ningún gap supera el umbral
```

## 2. Diseño

- **Corpus:** 40 memorias: 8 hechos actuales, 8 versiones obsoletas (distractores léxicos) y 24
  distractores temáticos.
- **Calibración:** 8 consultas, no usadas en evaluación.
- **Evaluación:** 8 consultas distintas, con la misma respuesta esperada por tema.
- **Brazos:** A = top-5 fijo; B = k variable con el umbral congelado.
- **Repeticiones:** 3 por consulta y brazo; 24 ejecuciones por brazo.
- **Limpieza:** un workspace nuevo por brazo y repetición, con las 40 memorias reinsertadas.
- **Orden:** A/B aleatorizado por consulta y repetición.
- **Modelo:** `gemma4:31b-cloud`, temperatura 0, `max_tokens` 80.
- **Contaminación:** las respuestas del benchmark no se escriben en la memoria evaluada.

## 3. Calibración

| Umbral | k medio | Recall@k |
|---|---|---|
| **0,05** | **3,5** | **1,00** |
| 0,08 | — | — |
| 0,10 | — | — |
| 0,12 | — | — |
| 0,15 | — | — |
| 0,20 | — | — |
| 0,25 | — | — |
| 0,30 | — | — |

El grid eligió el **umbral mínimo (0,05)** con recall 1,00. Es una señal importante: en este
corpus BM25 **no presenta una rodilla clara**; el mejor compromiso es el más permisivo. Los
umbrales superiores no mejoraron recall y habrían bajado k, pero el criterio de calibración
prioriza recall y luego menor k.

## 4. Evaluación

| Brazo | k medio | Recall | Precision | Tokens in | Coste | Latencia | Calidad |
|---|---|---|---|---|---|---|---|
| A (top-5) | 5,00 | 1,00 | 0,20 | 133,1 | $0,001175 | 706 ms | 24/24 |
| B (Adaptive-k) | 2,79 | 1,00 | **0,55** | **119,9** | **$0,001058** | **523 ms** | 24/24 |

- **Tokens:** −9,9 %. **No alcanza el −30 % exigido.**
- **Recall:** igual (1,00).
- **Calidad:** igual (24/24).
- **Latencia:** −26 %.
- **Precision:** 0,20 → 0,55, porque k baja de 5 a 2,79 sin perder la evidencia.

## 5. Por consulta

| Consulta | A k | A tokens | A ok | B k | B tokens | B ok |
|---|---|---|---|---|---|---|
| eval_build | 5.00 | 133 | 3/3 | 2.00 | 115 | 3/3 |
| eval_cola | 5.00 | 141 | 3/3 | 2.00 | 128 | 3/3 |
| eval_fuente | 5.00 | 134 | 3/3 | 1.00 | 103 | 3/3 |
| eval_hash | 5.00 | 136 | 3/3 | 3.00 | 136 | 3/3 |
| eval_jwt | 5.00 | 134 | 3/3 | 1.00 | 101 | 3/3 |
| eval_puerto | 5.00 | 137 | 3/3 | 2.33 | 126 | 3/3 |
| eval_rust | 5.00 | 129 | 3/3 | 9.00 | 129 | 3/3 |
| eval_tests | 5.00 | 121 | 3/3 | 2.00 | 121 | 3/3 |

Las cifras por consulta son medias de 3 repeticiones; el detalle exacto está en el JSONL.

## 6. Veredicto

**No validado como reemplazo de top-5.** El criterio exigía una reducción de tokens ≥30 % y B se
queda en −9,9 %. La calidad y el recall no caen, y la latencia mejora, pero el objetivo de coste
no se cumple.

**Qué sí se aprende:**

1. Adaptive-k sobre BM25 reduce k (5 → 2,79) sin perder evidencia en este corpus.
2. La ganancia de tokens es pequeña porque el contexto de memoria ya era corto (~133 tokens);
   el ahorro grande de Adaptive-k en la literatura viene de contextos de retrieval largos
   (pasajes), no de 5 memorias de una línea.
3. El umbral calibrado es el mínimo del grid: BM25 no muestra una rodilla estable. Con coseno,
   la literatura encuentra curvas más informativas; con BM25, el gap es ruidoso.

**Rollback:** no se adopta. El runtime conserva `top_k = 5` por defecto; el parámetro añadido para
el experimento queda con default 5 y no altera el comportamiento.

## 7. Limitaciones

- Corpus sintético de 40 memorias y 8+8 consultas; no representa un workspace real.
- Un solo modelo y temperatura 0; sin varianza de modelo.
- 3 repeticiones, no 5.
- El retrieval es FTS5/BM25; los resultados no se transfieren a embeddings.
- La calibración prioriza recall y luego k; otro criterio podría elegir otro umbral.

## 8. Siguiente paso (no ejecutado)

Si se quiere insistir, el experimento correcto no es "otro umbral": es medir con un corpus donde
el contexto de memoria sea grande (20–50 memorias relevantes) y comparar **k absoluto** contra
**gap**, además de un umbral mínimo de score. Pero eso es otro experimento, y el veredicto actual
es que Adaptive-k sobre BM25 **no alcanza el criterio de coste**.

## 9. Anomalía observada: k puede subir por encima de 5

En `eval_rust`, B eligió **k = 9** porque el mayor gap normalizado apareció en la cola de la
lista. Sin un tope superior, Adaptive-k **no garantiza menos tokens** que top-5: puede elegir más
memorias si el gap está lejos de la cabeza. En este corpus ocurrió una vez de ocho.

Consecuencia para el diseño: si se retoma, la regla debe incluir un **máximo duro** (p. ej. 5) y
un **umbral absoluto de score**, no solo el gap relativo. Sin esas dos condiciones, el mecanismo
es tan capaz de inflar el contexto como de reducirlo.
