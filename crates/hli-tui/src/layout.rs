//! Layout de Agent Mode en un solo sitio.
//!
//! El dibujo y el cálculo del viewport consultan esta función; así un cambio
//! de alturas no desincroniza el scroll, el ratón ni el hit map.

use ratatui::layout::{Constraint, Direction, Layout, Rect};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AgentRegions {
    pub header: Rect,
    pub body: Rect,
    pub composer: Rect,
    /// Una sola línea, vacía en reposo. No es una barra de estado con datos
    /// fijos: solo aparece cuando hay algo que decir (actividad nueva, error).
    pub status: Rect,
}

/// Cabecera fija, cuerpo elástico y compositor de altura conocida.
/// `offer` añade una fila porque el aviso de vínculo tiene tres líneas.
pub fn agent_regions(area: Rect, offer: bool) -> AgentRegions {
    let composer_h = if offer { 5 } else { 3 };
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(3),
            Constraint::Length(composer_h),
            Constraint::Length(1),
        ])
        .split(area);
    AgentRegions {
        header: chunks[0],
        body: chunks[1],
        composer: chunks[2],
        status: chunks[3],
    }
}

/// Filas útiles del transcript dentro del cuerpo: el cuerpo lleva una regla
/// superior con título, y el resto es contenido.
pub fn body_inner_height(body: Rect) -> usize {
    body.height.saturating_sub(1) as usize
}

/// Primera fila de contenido, en coordenadas absolutas del terminal.
pub fn body_content_top(body: Rect) -> usize {
    (body.y + 1) as usize
}
