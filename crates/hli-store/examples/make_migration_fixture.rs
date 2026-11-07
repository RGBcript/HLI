//! Genera una fixture real del esquema anterior (versión 5) para pruebas y gate.

fn main() {
    let home = std::env::args()
        .nth(1)
        .expect("uso: make_migration_fixture <HLI_HOME>");
    let home = std::path::PathBuf::from(home);
    let _ = std::fs::remove_dir_all(&home);
    hli_store::fixtures::create_previous_fixture(&home).expect("fixture");
    println!("{}", home.display());
}
