# Contribuir a HLI

1. Abre un issue describiendo el cambio y el comportamiento esperado.
2. Crea una rama y mantén commits pequeños y con mensaje imperativo.
3. Antes de enviar:

```powershell
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
cargo deny check
```

4. Añade pruebas para cambios de comportamiento. Los gates externos deben seguir
   verdes; no se aceptan gates falsificados ni skips silenciosos.
5. Documenta límites y cambios de contrato en `docs/` y `CHANGELOG.md`.

No incluyas claves privadas, secretos, bases de datos ni artefactos de `dist/`.
