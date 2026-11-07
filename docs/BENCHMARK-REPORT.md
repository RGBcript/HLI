# HLI — Benchmark de proveedores y coste

Modelo real: `gemma4:31b-cloud` vía Ollama. Precio de referencia: $0.27/M input, $1.10/M output (no es el precio real de Ollama, que es 0).

| Tarea | Modo | Input | Output | Latencia ms | Reintentos | Calidad | Coste ref. USD |
|---|---|---|---|---|---|---|---|
| log | baseline | 1558 | 38 | 583 | 0 | ok | 0.000462 |
| memory | baseline | 40 | 15 | 479 | 0 | ok | 0.000027 |
| files | baseline | 43 | 24 | 453 | 0 | ok | 0.000038 |
| fix | baseline | 46 | 120 | 767 | 0 | ok | 0.000144 |
| integrate | baseline | 41 | 89 | 729 | 0 | ok | 0.000109 |
| log | hli | 44 | 35 | 612 | 1 | fallo | 0.000050 |
| memory | hli | 42 | 120 | 801 | 1 | fallo | 0.000143 |
| files | hli | 39 | 29 | 545 | 1 | fallo | 0.000042 |
| fix | hli | 47 | 46 | 642 | 0 | ok | 0.000063 |
| integrate | hli | 46 | 11 | 463 | 0 | ok | 0.000025 |
| log | routed | 373 | 200 | 1579 | 1 | fallo | 0.000321 |
| memory | routed | 141 | 95 | 1107 | 0 | ok | 0.000143 |
| files | routed | 144 | 103 | 1164 | 0 | ok | 0.000152 |
| fix | routed | 147 | 200 | 1401 | 0 | ok | 0.000260 |
| integrate | routed | 92 | 42 | 2613 | 0 | ok | 0.000071 |

- **baseline**: 1728 input, 286 output, 3011 ms, 0 reintentos, calidad 5/5, coste ref. $0.000781
- **hli**: 218 input, 241 output, 3063 ms, 3 reintentos, calidad 2/5, coste ref. $0.000324
- **routed**: 897 input, 640 output, 7864 ms, 1 reintentos, calidad 4/5, coste ref. $0.000946

## Análisis (leer antes de citar cifras)

- **Tokens:** HLI redujo la entrada un 87 % (1 728 → 218) y el coste de tokens un 58 %
  (0,000781 → 0,000324 USD de referencia).
- **Calidad:** HLI acertó 2/5 tareas frente a 5/5 del baseline, con 3 reintentos. El contexto
  reducido perdió información que el baseline sí enviaba.
- **Por qué:** el `ContextManifest` incluye eventos y memorias, pero **no incluye slices de
  artefactos ni el contenido de archivos**. Las tareas que dependen de un archivo completo no
  pueden resolverse con el manifiesto actual.
- **Routed:** 897 tokens de entrada, 4/5 de calidad y 0,000946 USD, un 21 % más caro que el
  baseline. Con un solo modelo disponible, enrutar significa pagar dos llamadas sin un modelo
  económico distinto.
- **Conclusión:** en este benchmark HLI **no ahorra dinero neto** ni mejora la calidad. Ahorra
  tokens, y ese ahorro se pierde en reintentos y en tareas que necesitan artefactos. La hipótesis
  de ahorro no queda confirmada; queda medida y desmentida en estas cinco tareas.

## Condiciones

- Modelo: `gemma4:31b-cloud` vía Ollama (plan gratuito). Coste real 0; la tarifa de referencia
  es $0,27/M input y $1,10/M output, declarada en `scripts/benchmark.py`.
- 5 tareas, un intento por modo, reintento solo si el criterio determinista falla.
- Reproducible con `python scripts/benchmark.py` (sobrescribe este archivo y pierde el análisis;
  consérvalo o cópialo antes).
- No se midió intervención humana ni tiempo de retrabajo, solo el reintento.
