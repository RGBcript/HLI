//! Captura de ratón y restauración del terminal.
//!
//! `MouseGuard` es RAII: activa `EnableMouseCapture` al entrar y lo desactiva
//! al salir por cualquier vía (normal, `q`, error o panic). El hook de pánico
//! también lo desactiva antes de restaurar la pantalla.

use std::io::{self, Write};

use crossterm::event::{DisableMouseCapture, EnableMouseCapture};
use crossterm::execute;

pub struct MouseGuard {
    enabled: bool,
}

impl MouseGuard {
    pub fn new(enabled: bool) -> io::Result<Self> {
        if enabled {
            execute!(io::stdout(), EnableMouseCapture)?;
        }
        Ok(Self { enabled })
    }

    pub fn enabled(&self) -> bool {
        self.enabled
    }
}

impl Drop for MouseGuard {
    fn drop(&mut self) {
        if self.enabled {
            let mut out = io::stdout();
            let _ = execute!(out, DisableMouseCapture);
            let _ = out.flush();
        }
    }
}

/// Desactiva la captura desde cualquier contexto, incluido el hook de pánico.
pub fn release_mouse() {
    let mut out = io::stdout();
    let _ = execute!(out, DisableMouseCapture);
    let _ = out.flush();
}

/// ¿Debe capturarse el ratón? `HLI_DISABLE_MOUSE=1` lo desactiva y conserva la
/// selección nativa de Windows Terminal.
pub fn mouse_enabled_from_env() -> bool {
    !matches!(
        std::env::var("HLI_DISABLE_MOUSE").as_deref(),
        Ok("1") | Ok("true") | Ok("yes")
    )
}
