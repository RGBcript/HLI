# HLI — Benchmark v3

Principal `gemma4:31b-cloud`, secundario `qwen2.5-coder:1.5b` (local). 5 repeticiones. Precios de referencia: principal $0.27/$1.10, secundario $0.05/$0.20 por millón.

| Tarea | Modo | Input p50 | p90 | Latencia p50 | Calidad | Coste p50 |
|---|---|---|---|---|---|---|
| log_big | baseline | 68540 | 68540 | 1512 | 5/5 | $0.018651 |
| log_big | hli_v2 | 553 | 553 | 495 | 5/5 | $0.000168 |
| log_big | routed | 16499 | 16500 | 8094 | 0/5 | $0.000951 |
| repo_15 | baseline | 510 | 510 | 466 | 5/5 | $0.000158 |
| repo_15 | hli_v2 | 647 | 647 | 534 | 5/5 | $0.000192 |
| repo_15 | routed | 511 | 511 | 932 | 0/5 | $0.000146 |
| thread_100 | baseline | 1222 | 1222 | 462 | 5/5 | $0.000348 |
| thread_100 | hli_v2 | 256 | 260 | 474 | 5/5 | $0.000081 |
| thread_100 | routed | 116 | 116 | 2221 | 0/5 | $0.000138 |

- **baseline**: input p50 1222 (p90 68540, min 510, max 68540, sd 31903); coste p50 $0.000348; calidad 15/15
- **hli_v2**: input p50 553 (p90 647, min 252, max 647, sd 167); coste p50 $0.000168; calidad 15/15
- **routed**: input p50 511 (p90 16499, min 108, max 16500, sd 7632); coste p50 $0.000146; calidad 0/15

## Análisis

- **log_big (111 534 chars):** baseline 68 540 tokens de entrada; HLI 553 (−99,2 %); coste
  $0,018651 → $0,000168 (111× menos), calidad 5/5. Es el caso donde el slice de ventana de error
  sustituye a un log completo.
- **repo_15 (15 archivos):** baseline 510; HLI 647 (+27 %), calidad 5/5. El slice del repo añade
  metadatos y el baseline ya recibía un repo compacto. Aquí HLI no ahorra.
- **thread_100 (100 eventos):** baseline 1 222; HLI 256 (−79 %), coste $0,000348 → $0,000081,
  calidad 5/5. La memoria sustituye al historial.
- **Global:** calidad 15/15 en ambos; coste p50 $0,000168 vs $0,000348 (−52 %). La media está
  dominada por `log_big`; el ahorro no es uniforme.
- **Routed:** 0/15. El secundario de 1,5B no sostiene la extracción; ver `ROUTING-REPORT.md`.

## Percentiles

La tabla muestra p50 y p90. La desviación del baseline (sd 31 903) refleja la mezcla de cargas;
la de HLI v2 es 167, porque el presupuesto estabiliza el tamaño del contexto.

## Conclusión

HLI v2 iguala la calidad del baseline en las tres cargas y reduce el coste total un 52 % en p50,
pero pierde en `repo_15` (+27 %). El ahorro depende de que la evidencia pueda sustituirse por
memoria o por una ventana de slice; cuando el baseline ya es compacto, HLI añade overhead.
