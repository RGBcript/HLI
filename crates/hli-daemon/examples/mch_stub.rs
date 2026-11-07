//! Módulo MCH mínimo usado por las pruebas de autostart del daemon.
//! Reutiliza el fixture del workspace para no duplicar el protocolo.

fn main() {
    hli_module_fixture::run();
}
