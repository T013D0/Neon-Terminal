//! Animación de inicio: el planeta aparece, el anillo entra girando y todo se
//! desvanece hacia la terminal. Dura poco y no bloquea nada (la shell arranca
//! en paralelo y el overlay deja pasar los clics).

use eframe::egui::{
    self, Align2, Area, Color32, FontId, Id, Order, Pos2, Rect, Sense, TextureHandle, Vec2, pos2,
    vec2,
};
use std::time::Instant;

use crate::theme::pal;

const TOTAL: f32 = 1.35;

pub struct Intro {
    start: Instant,
    planet: TextureHandle,
    ring: TextureHandle,
}

fn load(ctx: &egui::Context, name: &str, bytes: &[u8]) -> Option<TextureHandle> {
    let icon = eframe::icon_data::from_png_bytes(bytes).ok()?;
    let img = egui::ColorImage::from_rgba_unmultiplied(
        [icon.width as usize, icon.height as usize],
        &icon.rgba,
    );
    Some(ctx.load_texture(name, img, egui::TextureOptions::LINEAR))
}

fn ease_out(t: f32) -> f32 {
    1.0 - (1.0 - t.clamp(0.0, 1.0)).powi(3)
}

/// Progreso (0..1) de un tramo [a, b] del tiempo.
fn seg(t: f32, a: f32, b: f32) -> f32 {
    ((t - a) / (b - a)).clamp(0.0, 1.0)
}

impl Intro {
    pub fn new(ctx: &egui::Context) -> Option<Self> {
        Some(Self {
            start: Instant::now(),
            planet: load(ctx, "intro_planet", include_bytes!("../assets/planet.png"))?,
            ring: load(ctx, "intro_ring", include_bytes!("../assets/ring.png"))?,
        })
    }

    /// Dibuja un cuadro; devuelve false cuando terminó.
    pub fn show(&self, ctx: &egui::Context) -> bool {
        let t = self.start.elapsed().as_secs_f32();
        if t >= TOTAL {
            return false;
        }
        ctx.request_repaint();
        let p = pal();
        let screen = ctx.screen_rect();
        let center = screen.center() - vec2(0.0, 10.0);

        let fade_out = 1.0 - ease_out(seg(t, 0.95, TOTAL));
        let bg_a = fade_out;
        let size = (screen.width().min(screen.height()) * 0.24).clamp(90.0, 170.0);
        let exit_scale = 1.0 + 0.07 * ease_out(seg(t, 0.95, TOTAL));

        // Planeta: aparece creciendo.
        let pa = ease_out(seg(t, 0.0, 0.35));
        let planet_size = size * (0.7 + 0.3 * pa) * exit_scale;
        // Anillo: entra girando.
        let ra = ease_out(seg(t, 0.15, 0.45));
        let rot = -2.4 * (1.0 - ease_out(seg(t, 0.15, 0.8)));
        let ring_size = size * exit_scale;
        // Brillo que respira al terminar de armarse el logo.
        let glow = (seg(t, 0.55, 0.95) * std::f32::consts::PI).sin().max(0.0);
        // Nombre.
        let na = ease_out(seg(t, 0.5, 0.85));

        Area::new(Id::new("neon_intro"))
            .order(Order::Foreground)
            .fixed_pos(screen.min)
            .interactable(false)
            .show(ctx, |ui| {
                ui.set_min_size(screen.size());
                let painter = ui.painter();
                painter.rect_filled(screen, 0.0, p.bg.gamma_multiply(bg_a));

                // Halo.
                for k in 0..7 {
                    let f = k as f32 / 7.0;
                    painter.circle_filled(
                        center,
                        size * (0.50 + 0.10 * k as f32) * exit_scale,
                        p.red
                            .gamma_multiply(0.045 * (1.0 - f) * (0.35 + glow) * pa * fade_out),
                    );
                }

                let rect = |s: f32| Rect::from_center_size(center, Vec2::splat(s));
                egui::Image::new(&self.planet)
                    .tint(Color32::WHITE.gamma_multiply(pa * fade_out))
                    .paint_at(ui, rect(planet_size));
                egui::Image::new(&self.ring)
                    .tint(Color32::WHITE.gamma_multiply(ra * fade_out))
                    .rotate(rot, vec2(0.5, 0.5))
                    .paint_at(ui, rect(ring_size));

                // Nombre con letras separadas.
                let letters = ['N', 'E', 'O', 'N'];
                let font = FontId::monospace(15.0);
                let gap = 22.0;
                let y = center.y + size * 0.5 + 34.0;
                let x0 = center.x - gap * (letters.len() as f32 - 1.0) / 2.0;
                for (i, c) in letters.iter().enumerate() {
                    let delay = i as f32 * 0.05;
                    let a = ease_out(seg(t, 0.5 + delay, 0.85 + delay)) * fade_out;
                    let pos: Pos2 = pos2(x0 + gap * i as f32, y + (1.0 - a) * 6.0);
                    painter.text(
                        pos,
                        Align2::CENTER_CENTER,
                        c.to_string(),
                        font.clone(),
                        p.text.gamma_multiply(a * (0.6 + 0.4 * na)),
                    );
                }
                let _ = Sense::hover();
            });
        true
    }
}
