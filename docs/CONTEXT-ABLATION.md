# HLI — Ablaciones del Context Engine

Carga: log de 111534 caracteres. 3 repeticiones. Se quita un componente cada vez.

| Variante | Input p50 | Calidad | Coste p50 |
|---|---|---|---|
| full | 553 | 3/3 | $0.000168 |
| no_memories | 534 | 3/3 | $0.000195 |
| no_events | 553 | 3/3 | $0.000168 |
| no_slices | 63 | 3/3 | $0.000036 |
| no_policy | 4582 | 3/3 | $0.001256 |

## Lectura

- **full** (553 tokens): memoria + slice + 2 eventos + política.
- **no_slices** (63): el modelo sigue acertando **sin el slice**, porque la memoria ya contiene la
  respuesta ("db timeout after 30s pool=8"). En esta carga el slice es redundante.
- **no_memories** (534): el slice provee la evidencia; el coste sube por más output.
- **no_events** (553): los eventos no aportan nada en esta carga.
- **no_policy** (4 582): sin presupuesto ni truncado, el contexto se multiplica por 8.

## Qué componente aporta qué

| Componente | Valor observado |
|---|---|
| Política adaptativa | **el que más ahorra**: 4 582 → 553 (−88 %) |
| Memoria | provee la respuesta en `memory_lookup` y `log_big` |
| Slice | necesario cuando no hay memoria (repo); redundante cuando la memoria ya responde |
| Eventos recientes | sin efecto medible en estas cargas |

## Conclusión

El contexto mínimo no es "todos los componentes": es **política + una fuente de evidencia**
(memoria o slice, no ambas). Añadir slices cuando la memoria ya contiene la respuesta cuesta
490 tokens y no mejora la calidad. La decisión debe depender de la tarea, no de una regla fija.
