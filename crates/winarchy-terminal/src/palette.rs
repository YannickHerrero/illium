use alacritty_terminal::vte::ansi::{Color, NamedColor, Rgb};
use winarchy_theme::Theme;
#[derive(Clone)]
pub struct Palette {
    pub colors: [Rgb; 269],
    pub selection: Rgb,
    pub opacity: f32,
}
fn rgb(hex: &str) -> Rgb {
    let (r, g, b) = winarchy_theme::rgb(hex).unwrap_or((255, 0, 255));
    Rgb { r, g, b }
}
impl Palette {
    pub fn new(theme: &Theme) -> Self {
        let fallback = if theme.mode.as_deref() == Some("light") {
            Theme::parse(include_str!("../../../config/themes/catppuccin-latte.toml")).unwrap()
        } else {
            Theme::default_theme()
        };
        let mut colors = [rgb(&theme.text); 269];
        for (offset, values) in [
            (0, theme.ansi.as_ref().or(fallback.ansi.as_ref()).unwrap()),
            (
                8,
                theme
                    .brights
                    .as_ref()
                    .or(fallback.brights.as_ref())
                    .unwrap(),
            ),
        ] {
            for (i, value) in values.iter().enumerate() {
                colors[offset + i] = rgb(value);
            }
        }
        let levels = [0, 95, 135, 175, 215, 255];
        for i in 0..216 {
            colors[16 + i] = Rgb {
                r: levels[i / 36],
                g: levels[i / 6 % 6],
                b: levels[i % 6],
            };
        }
        for i in 0..24 {
            let v = 8 + i as u8 * 10;
            colors[232 + i] = Rgb { r: v, g: v, b: v };
        }
        colors[NamedColor::Foreground as usize] = rgb(&theme.text);
        colors[NamedColor::Background as usize] = rgb(&theme.background);
        colors[NamedColor::Cursor as usize] = rgb(&theme.accent);
        for i in 0..8 {
            let c = colors[i];
            colors[259 + i] = Rgb {
                r: c.r / 2,
                g: c.g / 2,
                b: c.b / 2,
            };
        }
        colors[NamedColor::BrightForeground as usize] = rgb(&theme.text);
        colors[NamedColor::DimForeground as usize] = rgb(&theme.subtext);
        Self {
            colors,
            selection: rgb(&theme.overlay),
            opacity: theme.terminal_background_opacity,
        }
    }
    pub fn resolve(
        &self,
        color: Color,
        overrides: &alacritty_terminal::term::color::Colors,
    ) -> Rgb {
        match color {
            Color::Spec(rgb) => rgb,
            Color::Indexed(i) => overrides[i as usize].unwrap_or(self.colors[i as usize]),
            Color::Named(i) => overrides[i].unwrap_or(self.colors[i as usize]),
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn application_palette_overrides_and_reset() {
        let mut m = crate::model::Model::new(crate::model::Size::new(10, 2), 0);
        let p = Palette::new(&Theme::default_theme());
        m.feed(b"\x1b]4;1;#123456\x1b\\");
        assert_eq!(
            p.resolve(Color::Indexed(1), m.term.colors()),
            Rgb {
                r: 0x12,
                g: 0x34,
                b: 0x56
            }
        );
        m.feed(b"\x1b]104;1\x1b\\");
        assert_eq!(p.resolve(Color::Indexed(1), m.term.colors()), p.colors[1]);
    }
    #[test]
    fn legacy_and_cube() {
        let mut t = Theme::default_theme();
        t.ansi = None;
        t.brights = None;
        let p = Palette::new(&t);
        assert_eq!(p.opacity, 0.85);
        t.terminal_background_opacity = 1.0;
        assert_eq!(Palette::new(&t).opacity, 1.0);
        assert_eq!(p.colors[16], Rgb { r: 0, g: 0, b: 0 });
        assert_eq!(
            p.colors[231],
            Rgb {
                r: 255,
                g: 255,
                b: 255
            }
        );
        assert_eq!(p.colors[255].r, 238);
        let c = Rgb { r: 1, g: 2, b: 3 };
        assert_eq!(p.resolve(Color::Spec(c), &Default::default()), c);
    }
}
