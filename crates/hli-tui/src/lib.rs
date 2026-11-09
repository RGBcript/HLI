pub mod app;
pub mod classic;
pub mod client;
pub mod daemon_async;
pub mod layout;
pub mod terminal;
pub mod ui;
pub mod viewport;

use std::sync::atomic::{AtomicBool, Ordering};

pub static PANIC_RESTORED: AtomicBool = AtomicBool::new(false);

/// Restaura el terminal antes de que el pánico imprima; evita dejar raw mode.
pub fn install_panic_hook() {
    let default = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        // Restaurar ratón antes que la pantalla: si el pánico ocurre dentro de
        // la TUI, Windows Terminal debe recuperar su selección nativa.
        crate::terminal::release_mouse();
        ratatui::restore();
        PANIC_RESTORED.store(true, Ordering::SeqCst);
        default(info);
    }));
}

pub fn panic_restored() -> bool {
    PANIC_RESTORED.load(Ordering::SeqCst)
}
