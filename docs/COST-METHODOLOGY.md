# HLI — Metodología de coste

**Fecha:** 2026-11-07

## Principio

HLI no fija precios en el binario. Los precios son configuración versionada del usuario,
porque cambian con el proveedor, el modelo y la fecha.

## Registro de precios

`$HLI_HOME/prices.yaml`:

```yaml
provider: ollama
model: gemma4:31b
currency: USD
input_per_million: 0
output_per_million: 0
cached_input_per_million: 0
effective_from: 2026-11-07
source: user-configured
```

- `provider` y `model` admiten `*` como comodín.
- `cached_input_per_million` es opcional.
- Sin entrada de precio, el coste se marca `priced: false` y vale 0; **no se inventa**.

## Fórmula

```
coste_input  = input_tokens  × input_per_million  / 1e6
coste_output = output_tokens × output_per_million / 1e6
coste_cache  = cached_tokens × cached_input_per_million / 1e6
coste_total  = coste_input + coste_output + coste_cache
```

Se guarda en `model.responded.cost` y en `receipt.cost`, con `source` y `effective_from`.

## Qué mide el benchmark

- Tokens reportados por el proveedor (no estimados si hay usage).
- Latencia total (sin streaming).
- Reintentos: una segunda llamada si la calidad determinista falla.
- Calidad: criterio verificable por tarea (contiene la decisión, el error, los archivos, etc.).

## Qué NO incluye todavía

- Coste de verificación humana.
- Coste de retrabajo completo (solo se cuenta el reintento, no el tiempo).
- Coste de modelos secundarios en el modo routed (sí se suman sus tokens).
- Coste de almacenamiento o de artefactos.
- Precios reales de Ollama (0 en el plan usado); el informe usa una tarifa de referencia
  declarada para comparar tokens con dinero.

## Regla de honestidad

Un resultado con menos tokens **no** implica menor coste total. Si el contexto reducido baja la
calidad, el reintento y el retrabajo pueden anular el ahorro. El informe de benchmark registra
calidad y reintentos junto a tokens y coste; si el enrutamiento sale más caro, se publica.
