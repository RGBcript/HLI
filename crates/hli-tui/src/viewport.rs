//! Modelo de viewport del transcript.
//!
//! El transcript se transforma primero en líneas renderizadas (incluyendo
//! wrapping y actividad expandida) y después se calcula la ventana visible.
//! El compositor, la cabecera y la barra de estado no forman parte del área
//! desplazable.

/// Ancho mínimo razonable para calcular wrapping.
pub const MIN_WIDTH: usize = 20;

/// Cuenta las líneas que ocupa un texto al envolverse en `width` columnas.
/// Es determinista y no depende del terminal: mismas entradas, mismo total.
pub fn wrapped_lines(text: &str, width: usize) -> usize {
    let width = width.max(MIN_WIDTH);
    text.split('\n')
        .map(|line| {
            let chars = line.chars().count();
            if chars == 0 { 1 } else { chars.div_ceil(width) }
        })
        .sum()
}

#[derive(Clone, Debug)]
pub struct TranscriptViewport {
    pub first_visible_line: usize,
    pub total_rendered_lines: usize,
    pub viewport_height: usize,
    pub follow_mode: bool,
    pub unseen_activity_count: usize,
    pub wheel_speed: usize,
}

impl Default for TranscriptViewport {
    fn default() -> Self {
        Self {
            first_visible_line: 0,
            total_rendered_lines: 0,
            viewport_height: 0,
            follow_mode: true,
            unseen_activity_count: 0,
            wheel_speed: 3,
        }
    }
}

impl TranscriptViewport {
    pub fn max_scroll(&self) -> usize {
        self.total_rendered_lines
            .saturating_sub(self.viewport_height)
    }

    pub fn clamp(&mut self) {
        self.first_visible_line = self.first_visible_line.min(self.max_scroll());
        if self.first_visible_line >= self.max_scroll() {
            self.follow_mode = true;
        }
    }

    /// Recalcula el total y la altura (resize o contenido nuevo). Si el
    /// usuario estaba siguiendo el final, la ventana se mantiene al final.
    pub fn sync(&mut self, total: usize, height: usize) {
        self.total_rendered_lines = total;
        self.viewport_height = height;
        if self.follow_mode {
            self.first_visible_line = self.max_scroll();
        } else {
            self.clamp();
        }
    }

    /// Desplazamiento del usuario: abandona el seguimiento salvo que llegue
    /// al final.
    pub fn scroll(&mut self, delta: isize) {
        let next = self.first_visible_line as isize + delta;
        self.first_visible_line = next.clamp(0, self.max_scroll() as isize) as usize;
        if self.first_visible_line >= self.max_scroll() {
            self.follow_mode = true;
            self.unseen_activity_count = 0;
        } else {
            self.follow_mode = false;
        }
    }

    pub fn page(&mut self, direction: i8) {
        let step = (self.viewport_height / 2).max(1) as isize;
        self.scroll(step * direction as isize);
    }

    pub fn to_start(&mut self) {
        self.first_visible_line = 0;
        self.follow_mode = false;
    }

    pub fn to_bottom(&mut self) {
        self.first_visible_line = self.max_scroll();
        self.follow_mode = true;
        self.unseen_activity_count = 0;
    }

    pub fn at_bottom(&self) -> bool {
        self.first_visible_line >= self.max_scroll()
    }

    /// Se llama tras añadir contenido. Con seguimiento activo la ventana baja;
    /// sin él, la posición histórica no se mueve y se cuentan las novedades.
    pub fn on_content(&mut self, added_lines: usize) {
        if added_lines == 0 {
            return;
        }
        self.total_rendered_lines += added_lines;
        if self.follow_mode {
            self.first_visible_line = self.max_scroll();
        } else {
            self.unseen_activity_count += added_lines;
            self.clamp_preserving_position();
        }
    }

    fn clamp_preserving_position(&mut self) {
        // Al crecer el contenido, el máximo crece: la posición se conserva.
        if self.first_visible_line > self.max_scroll() {
            self.first_visible_line = self.max_scroll();
        }
    }

    /// Proporción visible [0,1] para la scrollbar.
    pub fn thumb_ratio(&self) -> f32 {
        if self.total_rendered_lines == 0 {
            return 1.0;
        }
        (self.viewport_height as f32 / self.total_rendered_lines as f32).min(1.0)
    }
}

/// Velocidad válida: entero 1..=20. Cualquier otro valor cae al default.
pub fn parse_wheel_speed(raw: Option<&str>) -> usize {
    raw.and_then(|v| v.trim().parse::<usize>().ok())
        .filter(|v| (1..=20).contains(v))
        .unwrap_or(3)
}

/// Configuración de desplazamiento: `HLI_WHEEL_SCROLL_SPEED` (1..=20).
pub fn wheel_speed_from_env() -> usize {
    parse_wheel_speed(std::env::var("HLI_WHEEL_SCROLL_SPEED").ok().as_deref())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vp(total: usize, height: usize) -> TranscriptViewport {
        let mut v = TranscriptViewport::default();
        v.sync(total, height);
        v
    }

    #[test]
    fn max_scroll_is_total_minus_height() {
        assert_eq!(vp(100, 20).max_scroll(), 80);
        assert_eq!(vp(10, 20).max_scroll(), 0);
    }

    #[test]
    fn scroll_clamps_at_bounds() {
        let mut v = vp(100, 20);
        v.to_start();
        v.scroll(-50);
        assert_eq!(v.first_visible_line, 0);
        v.scroll(10_000);
        assert_eq!(v.first_visible_line, 80);
    }

    #[test]
    fn scrolling_up_disables_follow() {
        let mut v = vp(100, 20);
        assert!(v.follow_mode);
        v.scroll(-3);
        assert!(!v.follow_mode);
        assert_eq!(v.first_visible_line, 77);
    }

    #[test]
    fn wheel_to_bottom_reenables_follow() {
        let mut v = vp(100, 20);
        v.scroll(-9);
        assert!(!v.follow_mode);
        v.scroll(9);
        assert!(v.follow_mode);
        assert_eq!(v.first_visible_line, 80);
    }

    #[test]
    fn new_activity_preserves_position_and_counts() {
        let mut v = vp(100, 20);
        v.to_start();
        v.scroll(30);
        let before = v.first_visible_line;
        v.on_content(5);
        assert_eq!(v.first_visible_line, before);
        assert_eq!(v.unseen_activity_count, 5);
        assert_eq!(v.total_rendered_lines, 105);
    }

    #[test]
    fn follow_mode_keeps_bottom_on_new_content() {
        let mut v = vp(100, 20);
        v.on_content(5);
        assert_eq!(v.first_visible_line, 85);
        assert_eq!(v.unseen_activity_count, 0);
    }

    #[test]
    fn jump_to_bottom_clears_unseen() {
        let mut v = vp(100, 20);
        v.scroll(-10);
        v.on_content(4);
        assert!(v.unseen_activity_count > 0);
        v.to_bottom();
        assert!(v.follow_mode);
        assert_eq!(v.unseen_activity_count, 0);
        assert_eq!(v.first_visible_line, v.max_scroll());
    }

    #[test]
    fn page_scrolls_half_viewport() {
        let mut v = vp(100, 20);
        v.page(-1);
        assert_eq!(v.first_visible_line, 70);
        v.page(1);
        assert_eq!(v.first_visible_line, 80);
    }

    #[test]
    fn resize_recalculates_and_clamps() {
        let mut v = vp(100, 20);
        v.scroll(-10);
        v.sync(100, 60);
        assert!(v.first_visible_line <= v.max_scroll());
        v.sync(40, 60);
        assert_eq!(v.max_scroll(), 0);
        assert_eq!(v.first_visible_line, 0);
        assert!(v.follow_mode);
    }

    #[test]
    fn wrapped_lines_respects_width_and_newlines() {
        assert_eq!(wrapped_lines("abc", 80), 1);
        assert_eq!(wrapped_lines(&"x".repeat(180), 80), 3);
        assert_eq!(wrapped_lines("a\nb\n", 80), 3);
        assert_eq!(wrapped_lines("", 80), 1);
    }

    #[test]
    fn invalid_scroll_speed_uses_default() {
        assert_eq!(parse_wheel_speed(Some("5")), 5);
        assert_eq!(parse_wheel_speed(Some("20")), 20);
        assert_eq!(parse_wheel_speed(Some("0")), 3);
        assert_eq!(parse_wheel_speed(Some("21")), 3);
        assert_eq!(parse_wheel_speed(Some("abc")), 3);
        assert_eq!(parse_wheel_speed(None), 3);
    }

    #[test]
    fn thumb_ratio_reflects_visible_proportion() {
        assert_eq!(vp(100, 50).thumb_ratio(), 0.5);
        assert_eq!(vp(10, 50).thumb_ratio(), 1.0);
        assert_eq!(vp(0, 50).thumb_ratio(), 1.0);
    }
}
