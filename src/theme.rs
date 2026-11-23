//! Estilo de la app: tema actual, fuentes, estilo de egui y coloreado de comandos.
//! Los colores vienen del tema elegido (ver `themes.rs`).

use crate::themes::{Palette, Theme};
use egui::{Color32, CornerRadius, FontData, FontDefinitions, FontFamily, Stroke};
use egui_term::{ColorPalette, TerminalTheme};
use std::sync::{Arc, RwLock};

/// Tema actual (se puede cambiar en Ajustes sin reiniciar).
static CURRENT: RwLock<Option<(Theme, Palette)>> = RwLock::new(None);

/// Colores del tema actual.
pub fn pal() -> Palette {
    if let Some((_, p)) = CURRENT.read().unwrap().as_ref() {
        return *p;
    }
    let t = crate::themes::builtin().remove(0);
    let p = t.palette();
    *CURRENT.write().unwrap() = Some((t, p));
    p
}

pub fn current_theme() -> Theme {
    pal();
    CURRENT.read().unwrap().as_ref().unwrap().0.clone()
}

pub fn set_theme(t: Theme) {
    let p = t.palette();
    *CURRENT.write().unwrap() = Some((t, p));
}

pub const RADIUS: u8 = 10;
pub const TERM_FONT_SIZE: f32 = 14.0;

/// Fondo de "cápsula": el color mezclado suavemente sobre el panel.
pub fn tint(c: Color32) -> Color32 {
    mix(pal().panel, c, 0.14)
}

pub fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgb(l(a.r(), b.r()), l(a.g(), b.g()), l(a.b(), b.b()))
}

pub fn hex(c: Color32) -> String {
    format!("#{:02x}{:02x}{:02x}", c.r(), c.g(), c.b())
}

/// Colores de la terminal según el tema. `opacity` < 1 deja ver el fondo
/// de la ventana (Mica/Acrylic o el escritorio).
pub fn terminal_theme(opacity: f32) -> TerminalTheme {
    let t = current_theme();
    let p = pal();
    let a = &t.ansi;
    let s = |x: &str| x.to_string();
    let alpha = (opacity.clamp(0.0, 1.0) * 255.0).round() as u8;
    let bg = if alpha == 255 {
        hex(p.panel)
    } else {
        format!("{}{:02x}", hex(p.panel), alpha)
    };
    let dim = |x: &str| {
        crate::themes::parse_hex(x)
            .map(|c| hex(mix(c, p.panel, 0.35)))
            .unwrap_or_else(|| s(x))
    };
    TerminalTheme::new(Box::new(ColorPalette {
        foreground: hex(p.text),
        background: bg,
        black: s(&a[0]),
        red: s(&a[1]),
        green: s(&a[2]),
        yellow: s(&a[3]),
        blue: s(&a[4]),
        magenta: s(&a[5]),
        cyan: s(&a[6]),
        white: s(&a[7]),
        bright_black: s(&a[8]),
        bright_red: s(&a[9]),
        bright_green: s(&a[10]),
        bright_yellow: s(&a[11]),
        bright_blue: s(&a[12]),
        bright_magenta: s(&a[13]),
        bright_cyan: s(&a[14]),
        bright_white: s(&a[15]),
        bright_foreground: None,
        dim_foreground: hex(mix(p.text, p.panel, 0.35)),
        dim_black: dim(&a[0]),
        dim_red: dim(&a[1]),
        dim_green: dim(&a[2]),
        dim_yellow: dim(&a[3]),
        dim_blue: dim(&a[4]),
        dim_magenta: dim(&a[5]),
        dim_cyan: dim(&a[6]),
        dim_white: dim(&a[7]),
    }))
}

fn load(paths: &[&str]) -> Option<Vec<u8>> {
    paths.iter().find_map(|p| std::fs::read(p).ok())
}

/// Cascadia para la terminal, Segoe UI para la interfaz (con respaldo
/// japonés por si aparece texto en kanji). Si no existen, usa equivalentes.
pub fn install_fonts(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    let add = |fonts: &mut FontDefinitions, name: &str, bytes: Vec<u8>| {
        fonts
            .font_data
            .insert(name.into(), Arc::new(FontData::from_owned(bytes)));
    };

    // Terminal: Cascadia Mono con íconos Nerd Font, incluida en el programa
    // (así el prompt con cápsulas e íconos se ve igual en cualquier PC).
    add(
        &mut fonts,
        "mono",
        include_bytes!("../assets/fonts/CaskaydiaMonoNerdFontMono-Regular.ttf").to_vec(),
    );
    fonts
        .families
        .entry(FontFamily::Monospace)
        .or_default()
        .insert(0, "mono".into());
    if let Some(b) = load(&[
        r"C:\Windows\Fonts\segoeui.ttf",
        "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
    ]) {
        add(&mut fonts, "ui", b);
        fonts
            .families
            .entry(FontFamily::Proportional)
            .or_default()
            .insert(0, "ui".into());
    }
    // La interfaz también puede mostrar los íconos de la Nerd Font.
    fonts
        .families
        .entry(FontFamily::Proportional)
        .or_default()
        .push("mono".into());
    // Símbolos que usa Claude Code (⏺ ⎿ ✻ ✳ ❯ …) y que la Nerd Font no trae.
    if let Some(b) = load(&[
        r"C:\Windows\Fonts\seguisym.ttf",
        "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
    ]) {
        add(&mut fonts, "symbols", b);
        for fam in [FontFamily::Proportional, FontFamily::Monospace] {
            fonts
                .families
                .entry(fam)
                .or_default()
                .push("symbols".into());
        }
    }
    if let Some(b) = load(&[
        r"C:\Windows\Fonts\YuGothR.ttc",
        r"C:\Windows\Fonts\meiryo.ttc",
        "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
    ]) {
        add(&mut fonts, "cjk", b);
        for fam in [FontFamily::Proportional, FontFamily::Monospace] {
            fonts.families.entry(fam).or_default().push("cjk".into());
        }
    }
    ctx.set_fonts(fonts);
}

pub fn apply_style(ctx: &egui::Context) {
    let p = pal();
    let which = if p.dark {
        egui::Theme::Dark
    } else {
        egui::Theme::Light
    };
    ctx.set_theme(which);
    ctx.style_mut_of(which, |style| {
        let r = CornerRadius::same(8);
        style.visuals = if p.dark {
            egui::Visuals::dark()
        } else {
            egui::Visuals::light()
        };
        let v = &mut style.visuals;
        v.panel_fill = pal().bg;
        v.window_fill = pal().surface;
        v.extreme_bg_color = pal().panel;
        v.faint_bg_color = pal().surface;
        v.window_stroke = Stroke::new(1.0_f32, pal().border);
        v.window_corner_radius = CornerRadius::same(RADIUS);
        v.menu_corner_radius = CornerRadius::same(RADIUS);
        v.window_shadow = egui::Shadow::NONE;
        v.popup_shadow = egui::Shadow::NONE;
        v.selection.bg_fill = pal().red.gamma_multiply(0.30);
        v.selection.stroke = Stroke::new(1.0_f32, pal().red);
        v.override_text_color = Some(pal().text);
        v.hyperlink_color = pal().cyan;
        v.text_cursor.stroke = Stroke::new(2.0_f32, pal().red);
        for w in [
            &mut v.widgets.noninteractive,
            &mut v.widgets.inactive,
            &mut v.widgets.hovered,
            &mut v.widgets.active,
            &mut v.widgets.open,
        ] {
            w.corner_radius = r;
        }
        v.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, pal().border);
        v.widgets.inactive.weak_bg_fill = Color32::TRANSPARENT;
        // Rieles de sliders, casillas y radios: que se distingan del fondo.
        v.widgets.inactive.bg_fill = pal().border;
        v.widgets.inactive.fg_stroke = Stroke::new(1.0_f32, pal().text);
        v.widgets.hovered.weak_bg_fill = pal().hover;
        v.widgets.hovered.bg_stroke = Stroke::NONE;
        v.widgets.active.weak_bg_fill = pal().border;
        v.widgets.open.weak_bg_fill = pal().hover;
        style.spacing.item_spacing = egui::vec2(8.0, 6.0);
        style.spacing.button_padding = egui::vec2(10.0, 5.0);
    });
}

/// Colorea un comando como lo hace la línea de PowerShell:
/// comando en rojo, opciones en cian, textos entre comillas en verde,
/// variables en violeta, números en naranja, operadores en gris.
pub fn highlight_command(cmd: &str, font: egui::FontId) -> egui::text::LayoutJob {
    use egui::text::{LayoutJob, TextFormat};
    let mut job = LayoutJob::default();
    let fmt = |c: Color32| TextFormat {
        font_id: font.clone(),
        color: c,
        ..Default::default()
    };
    let mut expect_command = true;
    let mut chars = cmd.char_indices().peekable();
    while let Some(&(start, ch)) = chars.peek() {
        let (end, color) = if ch.is_whitespace() {
            let mut end = start;
            while let Some(&(i, c)) = chars.peek() {
                if !c.is_whitespace() {
                    break;
                }
                end = i + c.len_utf8();
                chars.next();
            }
            (end, pal().text)
        } else if ch == '"' || ch == '\'' {
            chars.next();
            let mut end = start + 1;
            for (i, c) in chars.by_ref() {
                end = i + c.len_utf8();
                if c == ch {
                    break;
                }
            }
            (end, pal().green)
        } else if matches!(ch, '|' | ';' | '&' | '>' | '<') {
            chars.next();
            expect_command = true;
            (start + ch.len_utf8(), pal().muted)
        } else {
            let mut end = start;
            while let Some(&(i, c)) = chars.peek() {
                if c.is_whitespace() || matches!(c, '|' | ';' | '"' | '\'') {
                    break;
                }
                end = i + c.len_utf8();
                chars.next();
            }
            let word = &cmd[start..end];
            let color = if expect_command {
                expect_command = false;
                pal().red
            } else if word.starts_with('-') {
                pal().cyan
            } else if word.starts_with('$') {
                pal().violet
            } else if word.chars().all(|c| c.is_ascii_digit() || c == '.') {
                pal().orange
            } else {
                pal().text
            };
            (end, color)
        };
        job.append(&cmd[start..end], 0.0, fmt(color));
    }
    job
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn highlights_each_token() {
        let job = highlight_command(
            "git commit -m \"hola mundo\" | grep x",
            egui::FontId::monospace(12.0),
        );
        let parts: Vec<(&str, Color32)> = job
            .sections
            .iter()
            .map(|s| (&job.text[s.byte_range.clone()], s.format.color))
            .filter(|(t, _)| !t.trim().is_empty())
            .collect();
        assert_eq!(
            parts,
            vec![
                ("git", pal().red),
                ("commit", pal().text),
                ("-m", pal().cyan),
                ("\"hola mundo\"", pal().green),
                ("|", pal().muted),
                ("grep", pal().red),
                ("x", pal().text),
            ]
        );
        assert_eq!(job.text, "git commit -m \"hola mundo\" | grep x");
    }
}
