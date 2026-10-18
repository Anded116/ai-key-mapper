use macroquad::prelude::*;

/// Font with Cyrillic glyphs (the built-in macroquad font is ASCII-only).
pub struct Txt {
    font: Option<Font>,
}

const FONT_CANDIDATES: [&str; 3] = [
    "C:/Windows/Fonts/consola.ttf",
    "C:/Windows/Fonts/segoeui.ttf",
    "C:/Windows/Fonts/arial.ttf",
];

impl Txt {
    pub async fn load() -> Self {
        for path in FONT_CANDIDATES {
            if let Ok(mut font) = load_ttf_font(path).await {
                font.set_filter(FilterMode::Linear);
                return Self { font: Some(font) };
            }
        }
        Self { font: None }
    }

    pub fn draw(&self, s: &str, x: f32, y: f32, size: f32, color: Color) {
        draw_text_ex(
            s,
            x,
            y,
            TextParams {
                font: self.font.as_ref(),
                font_size: size as u16,
                color,
                ..Default::default()
            },
        );
    }

    pub fn shadow(&self, s: &str, x: f32, y: f32, size: f32, color: Color) {
        self.draw(s, x + 1.5, y + 1.5, size, Color::new(0.0, 0.0, 0.0, color.a * 0.8));
        self.draw(s, x, y, size, color);
    }

    pub fn width(&self, s: &str, size: f32) -> f32 {
        measure_text(s, self.font.as_ref(), size as u16, 1.0).width
    }

    pub fn centered(&self, s: &str, cx: f32, y: f32, size: f32, color: Color) {
        let w = self.width(s, size);
        self.shadow(s, cx - w / 2.0, y, size, color);
    }
}
