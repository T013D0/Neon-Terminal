//! Temas: los colores de la interfaz y de la terminal.
//!
//! Hay temas incluidos (oscuros y uno claro) y se pueden importar esquemas de
//! Windows Terminal (el bloque JSON de "schemes" de su settings.json).

use egui::Color32;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Theme {
    pub name: String,
    pub dark: bool,
    /// Interfaz.
    pub bg: String,
    pub panel: String,
    pub surface: String,
    pub hover: String,
    pub border: String,
    pub text: String,
    pub muted: String,
    pub accent: String,
    /// Colores de texto (también se usan en el prompt).
    pub orange: String,
    pub yellow: String,
    pub green: String,
    pub cyan: String,
    pub blue: String,
    pub violet: String,
    /// Los 16 colores ANSI de la terminal: negro, rojo, verde, amarillo, azul,
    /// magenta, cian, blanco, y sus versiones brillantes.
    pub ansi: [String; 16],
}

/// Los mismos colores ya convertidos, para dibujar.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Palette {
    pub dark: bool,
    pub bg: Color32,
    pub panel: Color32,
    pub surface: Color32,
    pub hover: Color32,
    pub border: Color32,
    pub text: Color32,
    pub muted: Color32,
    pub red: Color32,
    pub orange: Color32,
    pub yellow: Color32,
    pub green: Color32,
    pub cyan: Color32,
    pub blue: Color32,
    pub violet: Color32,
}

pub fn parse_hex(s: &str) -> Option<Color32> {
    let h = s.trim().trim_start_matches('#');
    if h.len() != 6 {
        return None;
    }
    let v = u32::from_str_radix(h, 16).ok()?;
    Some(Color32::from_rgb((v >> 16) as u8, (v >> 8) as u8, v as u8))
}

pub fn hex(c: Color32) -> String {
    format!("#{:02x}{:02x}{:02x}", c.r(), c.g(), c.b())
}

impl Theme {
    pub fn palette(&self) -> Palette {
        let c = |s: &str, fallback: Color32| parse_hex(s).unwrap_or(fallback);
        let fb = if self.dark {
            Color32::from_gray(30)
        } else {
            Color32::from_gray(235)
        };
        let fg = if self.dark {
            Color32::from_gray(230)
        } else {
            Color32::from_gray(30)
        };
        Palette {
            dark: self.dark,
            bg: c(&self.bg, fb),
            panel: c(&self.panel, fb),
            surface: c(&self.surface, fb),
            hover: c(&self.hover, fb),
            border: c(&self.border, Color32::GRAY),
            text: c(&self.text, fg),
            muted: c(&self.muted, Color32::GRAY),
            red: c(&self.accent, Color32::RED),
            orange: c(&self.orange, Color32::from_rgb(255, 160, 100)),
            yellow: c(&self.yellow, Color32::YELLOW),
            green: c(&self.green, Color32::GREEN),
            cyan: c(&self.cyan, Color32::LIGHT_BLUE),
            blue: c(&self.blue, Color32::BLUE),
            violet: c(&self.violet, Color32::from_rgb(200, 150, 255)),
        }
    }
}

fn t(name: &str, dark: bool, ui: [&str; 14], ansi: [&str; 16]) -> Theme {
    let s = |x: &str| x.to_string();
    Theme {
        name: s(name),
        dark,
        bg: s(ui[0]),
        panel: s(ui[1]),
        surface: s(ui[2]),
        hover: s(ui[3]),
        border: s(ui[4]),
        text: s(ui[5]),
        muted: s(ui[6]),
        accent: s(ui[7]),
        orange: s(ui[8]),
        yellow: s(ui[9]),
        green: s(ui[10]),
        cyan: s(ui[11]),
        blue: s(ui[12]),
        violet: s(ui[13]),
        ansi: ansi.map(s),
    }
}

/// Temas que vienen con Neon. El primero es el predeterminado.
pub fn builtin() -> Vec<Theme> {
    vec![
        t(
            "Brasa",
            true,
            [
                "#0d0e12", "#15161c", "#1c1d25", "#22242e", "#24262f", "#e7e9f0", "#6e7385",
                "#ed4a5b", "#ff9e64", "#ffd479", "#7ee787", "#6bd5ff", "#82aaff", "#d6a2ff",
            ],
            [
                "#2a2c37", "#ed4a5b", "#7ee787", "#ffd479", "#82aaff", "#d6a2ff", "#6bd5ff",
                "#c9ccd8", "#6e7385", "#ff7a87", "#a2f5a9", "#ffe3a3", "#a9c4ff", "#e6c2ff",
                "#9fe4ff", "#ffffff",
            ],
        ),
        t(
            "Brasa claro",
            false,
            [
                "#e9e7e3", "#f7f6f3", "#ffffff", "#ecebe7", "#d9d6cf", "#22232a", "#7b7f8c",
                "#d43a4c", "#c8641f", "#a07800", "#2f8f3a", "#0f86a8", "#2f5fcf", "#8a4fd0",
            ],
            [
                "#22232a", "#d43a4c", "#2f8f3a", "#a07800", "#2f5fcf", "#8a4fd0", "#0f86a8",
                "#7b7f8c", "#5b5f6c", "#e0525f", "#3fa34a", "#b88a00", "#4573e0", "#9d63e3",
                "#1d99bd", "#a5a8b3",
            ],
        ),
        t(
            "Medianoche",
            true,
            [
                "#0b0f1a", "#111727", "#182036", "#1e2842", "#232d47", "#dfe6f5", "#6b7899",
                "#5aa9ff", "#ff9e64", "#f5d67b", "#7ce0a3", "#62d6e8", "#7aa2ff", "#c39bff",
            ],
            [
                "#1e2842", "#ff6b81", "#7ce0a3", "#f5d67b", "#7aa2ff", "#c39bff", "#62d6e8",
                "#c3cbe0", "#6b7899", "#ff8fa0", "#a1f0c0", "#ffe7a6", "#a6c1ff", "#dbc2ff",
                "#93e6f2", "#ffffff",
            ],
        ),
        t(
            "Sumi",
            true,
            [
                "#121110", "#161412", "#221f1b", "#2e2924", "#3a342d", "#e8dfcc", "#8a8072",
                "#d9482b", "#d6a858", "#c9a96e", "#98a86c", "#8aa8a0", "#7f9ab0", "#b08aa0",
            ],
            [
                "#2a2622", "#d9482b", "#98a86c", "#d6a858", "#7f9ab0", "#b08aa0", "#8aa8a0",
                "#cfc5b0", "#6b6258", "#ef6a4c", "#b3c486", "#e8c27a", "#9fb7cc", "#c9a3b9",
                "#a7c2ba", "#f4ecdb",
            ],
        ),
    ]
}

/// Importa un esquema de Windows Terminal. Acepta un esquema suelto
/// (`{"name": …, "background": …}`), una lista de esquemas, o el settings.json
/// completo (toma su lista "schemes").
pub fn import_windows_terminal(json: &str) -> Result<Vec<Theme>, String> {
    let v: serde_json::Value =
        serde_json::from_str(json.trim()).map_err(|e| format!("No es JSON válido: {e}"))?;
    let list: Vec<serde_json::Value> =
        if let Some(arr) = v.get("schemes").and_then(|s| s.as_array()) {
            arr.clone()
        } else if let Some(arr) = v.as_array() {
            arr.clone()
        } else {
            vec![v]
        };
    let mut out = vec![];
    for s in list {
        let g = |k: &str| s.get(k).and_then(|x| x.as_str()).and_then(parse_hex);
        let (Some(bg), Some(fg)) = (g("background"), g("foreground")) else {
            continue;
        };
        let name = s
            .get("name")
            .and_then(|x| x.as_str())
            .unwrap_or("Importado")
            .to_string();
        let keys = [
            "black",
            "red",
            "green",
            "yellow",
            "blue",
            "purple",
            "cyan",
            "white",
            "brightBlack",
            "brightRed",
            "brightGreen",
            "brightYellow",
            "brightBlue",
            "brightPurple",
            "brightCyan",
            "brightWhite",
        ];
        let ansi: Vec<Color32> = keys.iter().map(|k| g(k).unwrap_or(fg)).collect();
        out.push(from_terminal_colors(&name, bg, fg, &ansi));
    }
    if out.is_empty() {
        Err("No encontré ningún esquema con \"background\" y \"foreground\".".into())
    } else {
        Ok(out)
    }
}

/// Arma un tema completo de interfaz a partir de los colores de una terminal.
pub fn from_terminal_colors(name: &str, bg: Color32, fg: Color32, ansi: &[Color32]) -> Theme {
    let dark = luminance(bg) < 0.5;
    let mix = |a: Color32, b: Color32, t: f32| {
        let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
        Color32::from_rgb(l(a.r(), b.r()), l(a.g(), b.g()), l(a.b(), b.b()))
    };
    let shade = if dark { Color32::BLACK } else { Color32::WHITE };
    let a = |i: usize| ansi.get(i).copied().unwrap_or(fg);
    let mut th = Theme {
        name: name.to_string(),
        dark,
        bg: hex(mix(bg, shade, 0.35)),
        panel: hex(bg),
        surface: hex(mix(bg, fg, 0.06)),
        hover: hex(mix(bg, fg, 0.10)),
        border: hex(mix(bg, fg, 0.14)),
        text: hex(fg),
        muted: hex(mix(bg, fg, 0.5)),
        accent: hex(a(1)),
        orange: hex(mix(a(1), a(3), 0.5)),
        yellow: hex(a(3)),
        green: hex(a(2)),
        cyan: hex(a(6)),
        blue: hex(a(4)),
        violet: hex(a(5)),
        ansi: Default::default(),
    };
    for (i, slot) in th.ansi.iter_mut().enumerate() {
        *slot = hex(a(i));
    }
    th
}

fn luminance(c: Color32) -> f32 {
    (0.2126 * c.r() as f32 + 0.7152 * c.g() as f32 + 0.0722 * c.b() as f32) / 255.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_themes_parse() {
        for t in builtin() {
            let p = t.palette();
            assert_eq!(p.dark, t.dark, "{}", t.name);
            assert!(t.ansi.iter().all(|c| parse_hex(c).is_some()), "{}", t.name);
        }
    }

    #[test]
    fn imports_windows_terminal_scheme() {
        let json = r##"{"schemes":[{"name":"Campbell","background":"#0C0C0C","foreground":"#CCCCCC",
            "black":"#0C0C0C","red":"#C50F1F","green":"#13A10E","yellow":"#C19C00","blue":"#0037DA",
            "purple":"#881798","cyan":"#3A96DD","white":"#CCCCCC","brightBlack":"#767676",
            "brightRed":"#E74856","brightGreen":"#16C60C","brightYellow":"#F9F1A5",
            "brightBlue":"#3B78FF","brightPurple":"#B4009E","brightCyan":"#61D6D6","brightWhite":"#F2F2F2"}]}"##;
        let t = import_windows_terminal(json).unwrap();
        assert_eq!(t.len(), 1);
        assert_eq!(t[0].name, "Campbell");
        assert!(t[0].dark);
        assert_eq!(t[0].accent, "#c50f1f");
        assert_eq!(t[0].ansi[15], "#f2f2f2");
        assert!(import_windows_terminal("{}").is_err());
    }
}
