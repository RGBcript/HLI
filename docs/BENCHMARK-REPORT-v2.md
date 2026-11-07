# HLI — Benchmark v2

Modelo: `gemma4:31b-cloud` vía Ollama. 3 repeticiones por tarea y modo. Tarifa de referencia: $0.27/M input, $1.10/M output.

Routed: **not_evaluable** — no hay un segundo modelo generativo distinto y más barato.

| Tarea | Modo | Input | Output | Latencia ms | Calidad | Reintentos | Slices | Memorias | Coste ref. |
|---|---|---|---|---|---|---|---|---|---|
| log | baseline | 4060 | 130 | 820 | 3/3 | 0 | 0 | 0 | $0.003720 |
| log | hli_v1 | 124 | 38 | 985 | 0/3 | 3 | 0 | 1 | $0.000226 |
| log | hli_v2 | 4124 | 37 | 615 | 3/3 | 0 | 1 | 1 | $0.003463 |
| memory | baseline | 40 | 15 | 467 | 3/3 | 0 | 0 | 0 | $0.000082 |
| memory | hli_v1 | 208 | 3 | 487 | 3/3 | 0 | 0 | 2 | $0.000179 |
| memory | hli_v2 | 255 | 3 | 479 | 3/3 | 0 | 0 | 2 | $0.000216 |
| files | baseline | 43 | 24 | 485 | 3/3 | 0 | 0 | 0 | $0.000114 |
| files | hli_v1 | 112 | 56 | 1160 | 0/3 | 3 | 0 | 1 | $0.000276 |
| files | hli_v2 | 103 | 16 | 486 | 3/3 | 0 | 1 | 1 | $0.000136 |
| fix | baseline | 50 | 142 | 857 | 3/3 | 0 | 0 | 0 | $0.000510 |
| fix | hli_v1 | 69 | 14 | 472 | 3/3 | 0 | 0 | 1 | $0.000102 |
| fix | hli_v2 | 115 | 14 | 617 | 3/3 | 0 | 1 | 1 | $0.000139 |
| integrate | baseline | 41 | 89 | 690 | 3/3 | 0 | 0 | 0 | $0.000327 |
| integrate | hli_v1 | 283 | 29 | 628 | 3/3 | 0 | 0 | 2 | $0.000327 |
| integrate | hli_v2 | 322 | 29 | 601 | 3/3 | 0 | 1 | 2 | $0.000357 |

- **baseline**: input medio 846, calidad 15/15, reintentos 0, coste total $0.004753, latencia media 664 ms
- **hli_v1**: input medio 159, calidad 9/15, reintentos 6, coste total $0.001109, latencia media 746 ms
- **hli_v2**: input medio 983, calidad 15/15, reintentos 0, coste total $0.004311, latencia media 559 ms

## Análisis

- **Criterio principal:** HLI v2 alcanza **15/15**, igual que el baseline. Cumple.
- **Coste total:** v2 $0,004311 frente a $0,004753 del baseline, un **9,3 % menos**.
- **Latencia media:** 559 ms frente a 664 ms (−16 %).
- **No es uniforme:** v2 es más caro en `memory` (+163 %), `files` (+19 %) e `integrate` (+9 %),
  y más barato en `log` (−7 %) y `fix` (−73 %). El total favorece a v2, pero el margen está
  dentro de la variación de tres repeticiones.
- **HLI v1** (sin slices) cae a 9/15 con 6 reintentos: `log` y `files` fallan por falta de
  evidencia. Los slices son la causa directa de la recuperación de calidad.
- **Routed:** `not_evaluable`. Solo hay un modelo generativo real (`gemma4:31b-cloud`); usar el
  mismo modelo dos veces no mide selección. El control negativo de v1 se conserva.

## Conclusión honesta

HLI v2 **iguala la calidad del baseline** y reduce el coste total un 9 % en este conjunto, pero
no en todas las tareas. No se puede afirmar que HLI ahorre de forma general: el ahorro depende de
que la tarea necesite un slice grande (donde el baseline también lo envía) o de que el historial
sea sustituible por memoria. En tareas pequeñas con memoria, HLI puede costar más porque añade
metadatos y memorias que el baseline no necesita.

## Condiciones

- 5 tareas × 3 modos × 3 repeticiones = 45 ejecuciones.
- `gemma4:31b-cloud` vía Ollama, temperatura 0, `max_tokens` 160.
- Tarifa de referencia declarada, no precio real de Ollama.
- Reintento automático si el criterio determinista falla; su coste se suma.
- Sin intervención humana medida.
