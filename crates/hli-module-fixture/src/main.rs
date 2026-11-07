//! Binario del módulo MCH de prueba: la lógica vive en la librería para que
//! otros targets (por ejemplo, el ejemplo `mch_stub` del daemon) la reutilicen.

fn main() {
    hli_module_fixture::run();
}
