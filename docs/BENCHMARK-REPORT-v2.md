# HLI — Benchmark v2

Modelo: `gemma4:31b-cloud` vía Ollama. 3 repeticiones por tarea y modo. Tarifa de referencia: $0.27/M input, $1.10/M output.

Routed: **not_evaluable** — no hay un segundo modelo generativo distinto y más barato.

| Tarea | Modo | Input | Output | Latencia ms | Calidad | Reintentos | Slices | Memorias | Coste ref. |
|---|---|---|---|---|---|---|---|---|---|
| log | baseline | 4060 | 135 | 834 | 3/3 | 0 | 0 | 0 | $0.003736 |
| log | hli_v1 | 124 | 38 | 1059 | 0/3 | 3 | 0 | 1 | $0.000226 |
| log | hli_v2 | 4124 | 41 | 676 | 3/3 | 0 | 1 | 1 | $0.003476 |
| memory | baseline | 40 | 15 | 417 | 3/3 | 0 | 0 | 0 | $0.000082 |
| memory | hli_v1 | 210 | 3 | 478 | 3/3 | 0 | 0 | 2 | $0.000180 |
| memory | hli_v2 | 254 | 3 | 458 | 3/3 | 0 | 0 | 2 | $0.000216 |
| files | baseline | 43 | 24 | 783 | 3/3 | 0 | 0 | 0 | $0.000114 |
| files | hli_v1 | 112 | 56 | 1155 | 0/3 | 3 | 0 | 1 | $0.000276 |
| files | hli_v2 | 103 | 16 | 516 | 3/3 | 0 | 1 | 1 | $0.000136 |
| fix | baseline | 50 | 143 | 801 | 3/3 | 0 | 0 | 0 | $0.000512 |
| fix | hli_v1 | 69 | 14 | 485 | 3/3 | 0 | 0 | 1 | $0.000102 |
| fix | hli_v2 | 114 | 16 | 489 | 3/3 | 0 | 1 | 1 | $0.000145 |
| integrate | baseline | 41 | 89 | 770 | 3/3 | 0 | 0 | 0 | $0.000327 |
| integrate | hli_v1 | 282 | 29 | 590 | 3/3 | 0 | 0 | 2 | $0.000327 |
| integrate | hli_v2 | 322 | 28 | 534 | 3/3 | 0 | 1 | 2 | $0.000354 |

- **baseline**: input medio 846, calidad 15/15, reintentos 0, coste total $0.004772, latencia media 721 ms
- **hli_v1**: input medio 159, calidad 9/15, reintentos 6, coste total $0.001110, latencia media 753 ms
- **hli_v2**: input medio 983, calidad 15/15, reintentos 0, coste total $0.004327, latencia media 534 ms

## Análisis

- **Criterio principal:** HLI v2 alcanza **15/15**, igual que el baseline. Cumple.
- **Coste total:** v2 $0,004327 frente a $0,004772 del baseline, un **9,3 % menos**.
- **Latencia media:** 534 ms frente a 721 ms (−26 %).
- **No es uniforme:** v2 es más caro en tareas pequeñas (`memory`, `files`) porque añade
  memorias y metadatos que el baseline no necesita; es más barato donde el baseline paga output
  largo (`fix`) o donde el slice sustituye al historial (`log`).
- **HLI v1** (sin slices) cae a 9/15 con 6 reintentos: `log` y `files` fallan por falta de
  evidencia. Los slices son la causa directa de la recuperación de calidad.
- **Trayectoria visible:** el manifiesto guarda `visible_event_ids` (todos los eventos visibles
  de la rama) separado de `recent_events` (selección presupuestada). Así se conserva la
  evidencia de aislamiento sin inflar el contexto enviado.
- **Routed:** `not_evaluable`. Solo hay un modelo generativo real (`gemma4:31b-cloud`); usar el
  mismo modelo dos veces no mide selección. El control negativo de v1 se conserva.

## Conclusión honesta

HLI v2 **iguala la calidad del baseline** y reduce el coste total un 9 % en este conjunto, pero
no en todas las tareas. No se puede afirmar un ahorro general: en tareas pequeñas con memoria,
HLI cuesta más porque añade metadatos y memorias que el baseline no necesita. El margen del 9 %
está dentro de la variación de tres repeticiones.

## Condiciones

- 5 tareas × 3 modos × 3 repeticiones = 45 ejecuciones.
- `gemma4:31b-cloud` vía Ollama, temperatura 0, `max_tokens` 160.
- Tarifa de referencia declarada, no precio real de Ollama.
- Reintento automático si el criterio determinista falla; su coste se suma.
- Sin intervención humana medida.
