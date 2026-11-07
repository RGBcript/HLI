# HLI — Guía de proveedores

**Estado:** implementado en `7e62c9e`. Configuración pública en `providers.toml`.

## Comandos

```powershell
hli provider add <nombre> --kind <sim|ollama|openai> --base-url <url> --model <modelo> [--timeout <segundos>] [--secret-handle <handle>]
hli provider list
hli provider inspect <nombre>
hli provider test <nombre>
hli provider set-default <nombre>
hli provider remove <nombre>
```

`provider test` hace una llamada mínima (`ping`) y devuelve proveedor, modelo, latencia, usage y texto. Para `sim` no hace red.

## Archivo

`%HLI_HOME%\providers.toml`:

```toml
default_provider = "ollama-local"

[providers.ollama-local]
kind = "ollama"
base_url = "http://localhost:11434"
model = "gemma4:31b-cloud"
timeout_seconds = 120

[providers.remote]
kind = "openai"
base_url = "https://api.openai.com/v1"
model = "gpt-4o-mini"
timeout_seconds = 120
secret_handle = "openai-key"
```

La clave **no** se guarda en `providers.toml`. `secret_handle` apunta a `%HLI_HOME%\secrets\<handle>`, que ya usa el mecanismo existente. El valor no aparece en SQLite, eventos, manifests, receipts ni logs.

## Uso en runs

```powershell
hli run start --workspace <ws> --thread <th> --branch <br> --prompt "..." --provider ollama-local
```

Si se omite `--provider`, se usa `default_provider`. Si no hay default, `sim`.

Los flags `--base-url`, `--model`, `--timeout-secs` y `--secret` siguen funcionando y tienen prioridad sobre la configuración, para compatibilidad con 0.1–0.5.

## Validación

`provider add` rechaza:

- `kind` distinto de `sim`, `ollama` u `openai`.
- `base_url` vacía o que no empiece por `http://` o `https://`.
- `model` vacío.
- `timeout_seconds` fuera de `1..3600`.

Un proveedor inválido no se escribe: el archivo anterior queda intacto.
