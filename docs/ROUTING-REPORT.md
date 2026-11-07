# HLI — Informe de enrutamiento

**Fecha:** 2026-11-07 · **Alcance:** routed con un segundo modelo real.

## 1. Condiciones

| Rol | Modelo | Dónde | Precio de referencia |
|---|---|---|---|
| Principal | `gemma4:31b-cloud` | Ollama cloud (plan free) | $0,27/M input · $1,10/M output |
| Secundario | `qwen2.5-coder:1.5b` | Ollama local | $0,05/M input · $0,20/M output |

El secundario cumple las tres condiciones para evaluar routed: es distinto, es más barato (local,
coste real 0) y la tarea asignada (extracción) es compatible con su tamaño en teoría.

## 2. Resultado

| Tarea | Modo | Input p50 | Calidad | Coste p50 |
|---|---|---|---|---|
| log_big | baseline | 68 540 | 5/5 | $0,018651 |
| log_big | routed | 16 499 | **0/5** | $0,000951 |
| repo_15 | baseline | 510 | 5/5 | $0,000158 |
| repo_15 | routed | 511 | **0/5** | $0,000146 |
| thread_100 | baseline | 1 222 | 5/5 | $0,000348 |
| thread_100 | routed | 116 | **0/5** | $0,000138 |

**Calidad global de routed: 0/15.** Coste menor, resultado inaceptable.

## 3. Por qué falla

- El secundario de 1,5B no extrae la respuesta con fiabilidad cuando el contexto es grande
  (`log_big` recibe 16k tokens y devuelve un extracto vacío o irrelevante).
- El principal recibe ese extracto y responde sin la evidencia; el criterio determinista falla.
- En `thread_100`, el secundario devuelve 116 tokens de entrada y no encuentra la decisión; el
  principal no puede recuperarla.
- No es un problema de HLI: es un problema de asignar una tarea de extracción a un modelo que no
  la sostiene.

## 4. Conclusión

**Routed no se declara.** Se conserva como control negativo, con el mismo criterio que el
benchmark v1: no se puede afirmar ahorro por selección si la selección degrada la calidad.

Para que routed sea una opción real hacen falta:

1. un secundario con capacidad suficiente para la tarea asignada (no basta que sea barato);
2. un criterio de derivación por dificultad, no por tamaño del prompt;
3. una verificación de la extracción antes de pasarla al principal (coste adicional que debe
   contarse).

## 5. Lo que sí se puede afirmar

- El **mecanismo** de enrutamiento existe: dos modelos distintos, precios distintos, trazas
  separadas.
- El **beneficio** no está demostrado con este par de modelos. El routing a un modelo débil
  ahorra dinero y destruye calidad.
