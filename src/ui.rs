//! Piezas de interfaz: paneles, barra inferior, cápsulas, íconos, pestañas,
//! botones de ventana y filas del historial.

use crate::commands::Cmd;
use crate::history::Entry;
use crate::pane::Pane;
use crate::theme::{self, pal};
use egui::{
    Align, Align2, Color32, CornerRadius, FontId, Frame, Layout, Margin, Rect, RichText, Sense,
    Stroke, StrokeKind, UiBuilder, pos2, vec2,
};
use egui_term::{TerminalFont, TerminalTheme, TerminalView};
use egui_tiles::{TileId, UiResponse};

pub fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Pestaña tipo cápsula. La activa tiene fondo; el punto lleva el color de la
/// pestaña (rojo por defecto). Se puede arrastrar.
/// Marca de una pestaña (qué pasa en ella cuando no la estás mirando).
#[derive(Clone, Copy, PartialEq)]
pub enum Badge {
    None,
    Activity,
    Attention,
    Working,
    /// Claude Code o un comando terminó mientras mirabas otra pestaña.
    Done,
    /// Hay un comando corriendo (que no es Claude Code).
    Running,
    /// Un comando falló mientras mirabas otra pestaña.
    Failed,
}

impl Badge {
    /// Texto para el tooltip de la pestaña.
    pub fn hint(self) -> Option<&'static str> {
        match self {
            Badge::Attention => Some("Te necesita (Claude Code espera tu respuesta o hubo un aviso)"),
            Badge::Working => Some("Claude Code está trabajando…"),
            Badge::Done => Some("Terminó (un comando o Claude Code)"),
            Badge::Running => Some("Hay un comando corriendo"),
            Badge::Failed => Some("Un comando falló"),
            Badge::Activity => Some("Hay salida nueva"),
            Badge::None => None,
        }
    }
}

pub fn tab(
    ui: &mut egui::Ui,
    title: &str,
    active: bool,
    color: Option<Color32>,
    dragging: bool,
    badge: Badge,
) -> egui::Response {
    let title = truncate(title, 28);
    let text_color = if active { pal().text } else { pal().muted };
    let galley = ui
        .painter()
        .layout_no_wrap(title, FontId::proportional(13.0), text_color);
    let (rect, resp) =
        ui.allocate_exact_size(vec2(galley.size().x + 40.0, 30.0), Sense::click_and_drag());
    let p = ui.painter();
    if dragging {
        // Hueco donde estaba la pestaña mientras se arrastra.
        p.rect_stroke(
            rect,
            CornerRadius::same(8),
            Stroke::new(1.0_f32, pal().border),
            StrokeKind::Inside,
        );
        return resp;
    }
    if active {
        p.rect_filled(rect, CornerRadius::same(8), pal().surface);
        p.rect_stroke(
            rect,
            CornerRadius::same(8),
            Stroke::new(1.0_f32, pal().border),
            StrokeKind::Inside,
        );
    } else if resp.hovered() {
        p.rect_filled(rect, CornerRadius::same(8), pal().panel);
    }
    let dot_c = match (color, active) {
        (Some(c), _) => c,
        (None, true) => pal().red,
        (None, false) => pal().border,
    };
    let center = pos2(rect.min.x + 14.0, rect.center().y);
    match badge {
        Badge::None => {
            p.circle_filled(center, 3.5, dot_c);
        }
        Badge::Activity => {
            p.circle_filled(center, 3.5, pal().text);
        }
        Badge::Attention => {
            // Halo que late: pide atención.
            let t = ui.input(|i| i.time) as f32;
            let k = 0.5 + 0.5 * (t * 3.0).sin();
            p.circle_filled(center, 4.5 + 2.5 * k, pal().red.gamma_multiply(0.18 + 0.2 * k));
            p.circle_filled(center, 3.5, pal().red);
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(50));
        }
        Badge::Working => {
            // Arco que gira: está pensando.
            let t = ui.input(|i| i.time) as f32;
            let c = pal().orange;
            p.circle_stroke(center, 4.5, Stroke::new(1.5_f32, c.gamma_multiply(0.25)));
            let a0 = t * 6.0;
            let pts: Vec<egui::Pos2> = (0..=10)
                .map(|k| {
                    let a = a0 + k as f32 * 0.17;
                    center + vec2(a.cos(), a.sin()) * 4.5
                })
                .collect();
            p.add(egui::Shape::line(pts, Stroke::new(1.8_f32, c)));
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(33));
        }
        Badge::Running => {
            // Arco tenue que gira: algo corre, sin apuro.
            let t = ui.input(|i| i.time) as f32;
            let c = pal().muted;
            p.circle_stroke(center, 4.5, Stroke::new(1.5_f32, c.gamma_multiply(0.3)));
            let a0 = t * 4.0;
            let pts: Vec<egui::Pos2> = (0..=10)
                .map(|k| {
                    let a = a0 + k as f32 * 0.17;
                    center + vec2(a.cos(), a.sin()) * 4.5
                })
                .collect();
            p.add(egui::Shape::line(pts, Stroke::new(1.6_f32, c)));
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(50));
        }
        Badge::Failed => {
            p.circle_filled(center, 4.0, pal().red);
            p.line_segment([center + vec2(-1.6, -1.6), center + vec2(1.6, 1.6)], Stroke::new(1.3_f32, pal().bg));
            p.line_segment([center + vec2(-1.6, 1.6), center + vec2(1.6, -1.6)], Stroke::new(1.3_f32, pal().bg));
        }
        Badge::Done => {
            p.circle_filled(center, 4.0, pal().green);
            p.line_segment(
                [center + vec2(-2.0, 0.2), center + vec2(-0.6, 1.6)],
                Stroke::new(1.3_f32, pal().bg),
            );
            p.line_segment(
                [center + vec2(-0.6, 1.6), center + vec2(2.2, -1.4)],
                Stroke::new(1.3_f32, pal().bg),
            );
        }
    }
    if let (Some(c), true) = (color, active) {
        p.hline(
            (rect.min.x + 10.0)..=(rect.max.x - 10.0),
            rect.max.y - 1.0,
            Stroke::new(2.0_f32, c),
        );
    }
    p.galley(
        pos2(rect.min.x + 26.0, rect.center().y - galley.size().y / 2.0),
        galley,
        text_color,
    );
    resp
}

/// Pestaña "fantasma" que sigue al puntero mientras se arrastra.
pub fn tab_ghost(p: &egui::Painter, at: egui::Pos2, title: &str, color: Color32) {
    let title = truncate(title, 28);
    let galley = p.layout_no_wrap(title, FontId::proportional(13.0), pal().text);
    let rect = Rect::from_min_size(at + vec2(-20.0, -15.0), vec2(galley.size().x + 40.0, 30.0));
    p.rect_filled(
        rect,
        CornerRadius::same(8),
        pal().surface.gamma_multiply(0.95),
    );
    p.rect_stroke(
        rect,
        CornerRadius::same(8),
        Stroke::new(1.0_f32, color),
        StrokeKind::Inside,
    );
    p.circle_filled(pos2(rect.min.x + 14.0, rect.center().y), 3.5, color);
    p.galley(
        pos2(rect.min.x + 26.0, rect.center().y - galley.size().y / 2.0),
        galley,
        pal().text,
    );
}

// ─────────────────────────── Paneles ───────────────────────────

/// Lo que dura la animación al cerrar un panel.
pub const CLOSE_ANIM: std::time::Duration = std::time::Duration::from_millis(150);

pub struct PaneBehavior<'a> {
    pub focused: u64,
    pub input_enabled: bool,
    pub theme: &'a TerminalTheme,
    pub font: &'a TerminalFont,
    pub cmds: Vec<Cmd>,
    /// Dónde se dibujó cada panel (para navegar con Alt+flechas y soltar pestañas).
    pub rects: Vec<(u64, Rect)>,
    /// Opacidad del fondo de los paneles (1 = sólido).
    pub opacity: f32,
    pub copy_on_select: bool,
    pub middle_paste: bool,
    pub blocks: bool,
    /// Progreso (0..1) del cambio de pestaña; 1 = sin animación.
    pub switch: f32,
}

impl PaneBehavior<'_> {
    /// Selección que sobrevive al scroll de apps en pantalla alterna con mouse.
    /// Bloques de comandos: barra de estado a la izquierda, botones al pasar
    /// el mouse y el comando fijo arriba cuando su salida sigue en pantalla.
    fn blocks_ui(&mut self, ui: &mut egui::Ui, pane: &mut Pane, term_rect: Rect) {
        let views = pane.visible_blocks();
        if views.is_empty() {
            return;
        }
        let p = pal();
        let ch = pane.backend.last_content().terminal_size.cell_height as f32;
        if ch <= 0.0 {
            return;
        }
        let rows = (term_rect.height() / ch).floor() as i64;
        let y_of = |row: i64| term_rect.min.y + row.clamp(0, rows) as f32 * ch;
        let hover_row = ui
            .input(|i| i.pointer.hover_pos())
            .filter(|pos| term_rect.contains(*pos))
            .map(|pos| ((pos.y - term_rect.min.y) / ch).floor() as i64);
        let painter = ui.painter_at(term_rect.expand2(vec2(12.0, 0.0)));
        let status = |b: &crate::pane::Block| -> (Color32, String) {
            let secs = |d: std::time::Duration| {
                let s = d.as_secs_f32();
                if s < 1.0 {
                    format!("{} ms", d.as_millis())
                } else if s < 60.0 {
                    format!("{s:.1} s")
                } else {
                    format!("{}m {}s", s as u64 / 60, s as u64 % 60)
                }
            };
            match b.ok {
                None => (p.yellow, format!("● {}", secs(b.started.elapsed()))),
                Some(true) => (p.green, format!("✓ {}", b.duration.map(secs).unwrap_or_default())),
                Some(false) => (p.red, format!("✕ {}", b.duration.map(secs).unwrap_or_default())),
            }
        };
        let mut running = false;
        let mut sticky = None;
        for v in &views {
            let b = &pane.blocks[v.index];
            let (color, _) = status(b);
            running |= b.ok.is_none();
            let (top, bottom) = (y_of(v.top), y_of(v.bottom));
            // Barra de estado a la izquierda, con un corte entre bloques.
            if bottom - top > 2.0 {
                painter.rect_filled(
                    Rect::from_min_max(
                        pos2(term_rect.min.x - 9.0, top + 2.0),
                        pos2(term_rect.min.x - 7.0, bottom - 2.0),
                    ),
                    1.0,
                    color.gamma_multiply(0.75),
                );
            }
            if v.top > 0 {
                painter.hline(
                    term_rect.x_range(),
                    top - 0.5,
                    Stroke::new(1.0_f32, p.border.gamma_multiply(0.6)),
                );
            }
            if v.cmd_row < 0 && v.bottom > 1 {
                sticky = Some(v.clone());
            }
            // Mientras corre (Claude Code, vim…) los botones estorbarían.
            let hovered = b.ok.is_some() && hover_row.is_some_and(|r| r >= v.top && r < v.bottom);
            if hovered {
                painter.rect_filled(
                    Rect::from_min_max(pos2(term_rect.min.x - 4.0, top), pos2(term_rect.max.x, bottom)),
                    4.0,
                    p.text.gamma_multiply(0.035),
                );
                // Botones arriba a la derecha (bajo el encabezado fijo si lo hay).
                let y = if v.cmd_row < 0 {
                    term_rect.min.y + ch + 10.0
                } else {
                    y_of(v.cmd_row) + 1.0
                };
                let mut x = term_rect.max.x - 4.0;
                let idx = v.index;
                let failed = b.ok == Some(false);
                let mut buttons = vec![("Copiar salida", 1), ("Copiar comando", 0)];
                if failed {
                    buttons.push(("✳ Explicar el error", 2));
                }
                for (label, what) in buttons {
                    let font = egui::FontId::proportional(11.5);
                    let w = ui.fonts(|f| f.layout_no_wrap(label.into(), font.clone(), p.text).size().x) + 14.0;
                    let r = Rect::from_min_size(pos2(x - w, y), vec2(w, 20.0));
                    x -= w + 4.0;
                    let resp = ui.interact(r, ui.id().with(("block_btn", pane.id, idx, what)), Sense::click());
                    let bg = if resp.hovered() { p.hover } else { p.surface };
                    ui.painter().rect(r, 5.0, bg, Stroke::new(1.0_f32, p.border), egui::StrokeKind::Inside);
                    ui.painter().text(r.center(), egui::Align2::CENTER_CENTER, label, font, p.text);
                    if what == 2 {
                        if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                            self.cmds.push(Cmd::ExplainBlock(pane.id, idx));
                        }
                        continue;
                    }
                    if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                        let text = if what == 0 {
                            pane.blocks[idx].command.clone()
                        } else {
                            pane.block_output(idx)
                        };
                        ui.ctx().copy_text(text);
                        self.cmds.push(Cmd::Toast("Copiado".into()));
                    }
                }
            }
        }
        // Encabezado fijo: el comando cuya salida estás leyendo.
        if let Some(v) = sticky {
            let b = pane.blocks[v.index].clone();
            let (color, st) = status(&b);
            let r = Rect::from_min_size(term_rect.min - vec2(4.0, 0.0), vec2(term_rect.width() + 4.0, ch + 6.0));
            let resp = ui.interact(r, ui.id().with(("block_sticky", pane.id)), Sense::click());
            ui.painter().rect(
                r,
                CornerRadius { nw: 0, ne: 0, sw: 6, se: 6 },
                if resp.hovered() { p.hover } else { p.surface },
                Stroke::new(1.0_f32, p.border),
                egui::StrokeKind::Inside,
            );
            let mono = egui::FontId::monospace(12.5);
            ui.painter().text(
                pos2(r.min.x + 10.0, r.center().y),
                egui::Align2::LEFT_CENTER,
                format!("❯ {}", b.command),
                mono.clone(),
                p.text,
            );
            ui.painter().text(
                pos2(r.max.x - 10.0, r.center().y),
                egui::Align2::RIGHT_CENTER,
                st,
                egui::FontId::proportional(11.5),
                color,
            );
            if resp
                .on_hover_text("Ir al comando")
                .on_hover_cursor(egui::CursorIcon::PointingHand)
                .clicked()
            {
                pane.scroll_to(b.prompt - 1, false);
            }
        }
        if running {
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(500));
        }
    }

    /// Panel de cambios a la derecha del panel: archivos y diff del elegido.
    fn changes_ui(&mut self, ui: &mut egui::Ui, pane: &mut Pane, term_rect: Rect) {
        let p = pal();
        let base = pane.claude_base.clone();
        let pid = pane.id;
        let Some(ch) = pane.changes.as_mut() else { return };
        // Mientras está abierto se actualiza cada 3 s (un `git status`).
        if ch.loaded.elapsed() > std::time::Duration::from_secs(3) {
            ch.refresh(base.as_ref());
        }
        ui.ctx().request_repaint_after(std::time::Duration::from_secs(3));
        let width = (term_rect.width() * 0.55).clamp(320.0, 760.0).min(term_rect.width());
        let rect = Rect::from_min_max(pos2(term_rect.max.x - width, term_rect.min.y), term_rect.max);
        let mut close = false;
        let mut open_file = None;
        let mut discard = None;
        egui::Area::new(egui::Id::new(("changes", pid)))
            .order(egui::Order::Foreground)
            .fixed_pos(rect.min)
            .show(ui.ctx(), |ui| {
                popup_frame().inner_margin(egui::Margin::same(12)).show(ui, |ui| {
                    ui.set_width(width - 24.0);
                    ui.set_height(rect.height() - 26.0);
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Cambios").size(15.0).strong());
                        let n_claude = ch.files.iter().filter(|f| f.by_claude).count();
                        let summary = if base.is_some() {
                            format!("{} archivo(s) · {n_claude} desde que arrancó Claude", ch.files.len())
                        } else {
                            format!("{} archivo(s) sin commit", ch.files.len())
                        };
                        ui.label(RichText::new(summary).color(p.muted).size(12.0));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.small_button("✕").on_hover_text("Cerrar (Ctrl+Shift+G)").clicked() {
                                close = true;
                            }
                        });
                    });
                    ui.add_space(6.0);
                    if ch.files.is_empty() {
                        ui.label(RichText::new("No hay cambios sin commit.").color(p.muted));
                        return;
                    }
                    let list_h = (ch.files.len() as f32 * 24.0).min(rect.height() * 0.35);
                    let mut pick = None;
                    egui::ScrollArea::vertical()
                        .id_salt("files")
                        .max_height(list_h)
                        .show(ui, |ui| {
                            for (i, f) in ch.files.iter().enumerate() {
                                let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 22.0), Sense::click());
                                if ch.selected == Some(i) {
                                    ui.painter().rect_filled(r, CornerRadius::same(5), theme::mix(p.surface, p.red, 0.16));
                                } else if resp.hovered() {
                                    ui.painter().rect_filled(r, CornerRadius::same(5), p.hover);
                                }
                                let (letter, color) = match f.status.trim() {
                                    "??" => ("N", p.cyan),
                                    s if s.contains('D') => ("D", p.red),
                                    s if s.contains('A') => ("A", p.green),
                                    s if s.contains('R') => ("R", p.violet),
                                    _ => ("M", p.orange),
                                };
                                ui.painter().text(pos2(r.min.x + 10.0, r.center().y), egui::Align2::CENTER_CENTER, letter, FontId::monospace(12.0), color);
                                ui.painter().text(pos2(r.min.x + 22.0, r.center().y), egui::Align2::LEFT_CENTER, truncate(&f.path, 70), FontId::monospace(12.0), p.text);
                                if f.by_claude {
                                    ui.painter().text(pos2(r.max.x - 8.0, r.center().y), egui::Align2::RIGHT_CENTER, "✳ Claude", FontId::proportional(11.0), p.orange);
                                }
                                if resp.clicked() {
                                    pick = Some(i);
                                }
                            }
                        });
                    if let Some(i) = pick {
                        ch.select(i);
                    }
                    let Some(f) = ch.selected.and_then(|i| ch.files.get(i)).cloned() else {
                        ui.add_space(8.0);
                        ui.label(RichText::new("Elige un archivo para ver su diff.").color(p.muted));
                        return;
                    };
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(truncate(&f.path, 60)).monospace().strong());
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if f.restorable() {
                                if ch.confirm_discard.as_deref() == Some(f.path.as_str()) {
                                    let b = egui::Button::new(RichText::new("¿Seguro? Descartar").color(Color32::WHITE)).fill(p.red);
                                    if ui.add(b).clicked() {
                                        discard = Some(f.path.clone());
                                    }
                                } else if ui.button("Descartar").on_hover_text("Volver a como estaba en el último commit").clicked() {
                                    ch.confirm_discard = Some(f.path.clone());
                                }
                            }
                            if ui.button("Abrir").on_hover_text("Abrir en el editor").clicked() {
                                open_file = Some(ch.full_path(&f.path).display().to_string());
                            }
                        });
                    });
                    ui.add_space(4.0);
                    egui::ScrollArea::both().id_salt("diff").auto_shrink([false, false]).show(ui, |ui| {
                        for l in &ch.diff {
                            let color = if l.starts_with('+') {
                                p.green
                            } else if l.starts_with('-') {
                                p.red
                            } else if l.starts_with("@@") {
                                p.cyan
                            } else {
                                p.muted
                            };
                            ui.label(RichText::new(l).monospace().size(12.0).color(color));
                        }
                    });
                });
            });
        if let Some(path) = discard {
            match ch.discard(&path) {
                Ok(()) => self.cmds.push(Cmd::Toast(format!("Descartado: {path}"))),
                Err(e) => self.cmds.push(Cmd::Toast(e)),
            }
            ch.confirm_discard = None;
            ch.refresh(base.as_ref());
        }
        if let Some(f) = open_file {
            if let Some(err) = pane.open_link(&f) {
                self.cmds.push(Cmd::Toast(err));
            }
        }
        if close {
            pane.changes = None;
        }
    }

    /// Claude: aviso mientras escribe un comando y tarjeta con la explicación
    /// de un error.
    fn ai_ui(&mut self, ui: &mut egui::Ui, pane: &mut Pane, term_rect: Rect) {
        let p = pal();
        let t = ui.input(|i| i.time) as f32;
        if let Some(t0) = pane.ai_busy {
            let secs = t0.elapsed().as_secs();
            let text = format!("✳ Claude está escribiendo el comando…  {secs} s  ·  Esc cancela");
            let font = egui::FontId::proportional(12.0);
            let w = ui.fonts(|f| f.layout_no_wrap(text.clone(), font.clone(), p.text).size().x) + 22.0;
            let y = pane
                .cursor_pos()
                .map(|c| c.y + pane.backend.last_content().terminal_size.cell_height as f32 + 6.0)
                .unwrap_or(term_rect.max.y - 40.0)
                .min(term_rect.max.y - 28.0);
            let r = Rect::from_min_size(pos2(term_rect.min.x, y), vec2(w, 24.0));
            let k = 0.6 + 0.4 * (t * 3.0).sin();
            ui.painter().rect(r, 12.0, p.surface, Stroke::new(1.0_f32, p.orange.gamma_multiply(k)), egui::StrokeKind::Inside);
            ui.painter().text(r.center(), egui::Align2::CENTER_CENTER, text, font, p.text);
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(50));
        }
        let Some(note) = pane.ai_note.clone() else { return };
        let width = (term_rect.width() - 24.0).min(680.0);
        let mut close = false;
        egui::Area::new(egui::Id::new(("ai_note", pane.id)))
            .order(egui::Order::Foreground)
            .fixed_pos(pos2(term_rect.max.x - width - 4.0, term_rect.min.y + 8.0))
            .show(ui.ctx(), |ui| {
                popup_frame().inner_margin(egui::Margin::same(14)).show(ui, |ui| {
                    ui.set_width(width - 28.0);
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("✳").color(p.orange).size(15.0));
                        ui.label(RichText::new("Claude · por qué falló").strong());
                        ui.label(
                            RichText::new(crate::ui::truncate(&note.command, 40))
                                .monospace()
                                .color(p.muted)
                                .size(12.0),
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.small_button("✕").clicked() {
                                close = true;
                            }
                        });
                    });
                    ui.add_space(6.0);
                    match &note.body {
                        None => {
                            let secs = note.started.elapsed().as_secs();
                            ui.label(RichText::new(format!("Pensando… {secs} s")).color(p.muted));
                            ui.ctx().request_repaint_after(std::time::Duration::from_millis(200));
                        }
                        Some(body) => {
                            egui::ScrollArea::vertical().max_height(term_rect.height() * 0.5).show(ui, |ui| {
                                for line in body.lines() {
                                    if let Some(cmd) = line.trim().strip_prefix("$ ") {
                                        ui.horizontal(|ui| {
                                            ui.label(RichText::new(cmd).monospace().color(p.green));
                                            if ui.small_button("Escribir").on_hover_text("Lo deja escrito en la terminal").clicked() {
                                                self.cmds.push(Cmd::Insert(pane.id, cmd.to_string()));
                                                close = true;
                                            }
                                        });
                                    } else {
                                        ui.label(RichText::new(line).color(if note.error { p.red } else { p.text }));
                                    }
                                }
                            });
                            ui.add_space(8.0);
                            ui.horizontal(|ui| {
                                if ui.button("Copiar").clicked() {
                                    ui.ctx().copy_text(body.clone());
                                    self.cmds.push(Cmd::Toast("Copiado".into()));
                                }
                                if ui.button("Seguir en Claude Code").clicked() {
                                    self.cmds.push(Cmd::ExplainInClaude(pane.id));
                                }
                            });
                        }
                    }
                });
            });
        if close {
            pane.ai_note = None;
        }
    }

    /// Selección rápida: etiqueta sobre cada cosa copiable; escribir la
    /// etiqueta la copia (en mayúscula, con Shift, la escribe en la terminal).
    fn quick_ui(&mut self, ui: &mut egui::Ui, pane: &mut Pane, term_rect: Rect, focused: bool) {
        let p = pal();
        let size = &pane.backend.last_content().terminal_size;
        let (cw, ch) = (size.cell_width as f32, size.cell_height as f32);
        if focused {
            let mut chosen = None;
            let mut cancel = !self.input_enabled;
            ui.input(|i| {
                for e in &i.events {
                    match e {
                        egui::Event::Text(t) => {
                            for c in t.chars().filter(|c| c.is_ascii_alphabetic()) {
                                if let Some(q) = pane.quick.as_mut() {
                                    if let Some(text) = q.key(c) {
                                        chosen = Some((text, c.is_ascii_uppercase()));
                                    }
                                }
                            }
                        }
                        egui::Event::Key { key: egui::Key::Escape, pressed: true, .. } => cancel = true,
                        egui::Event::PointerButton { pressed: true, .. } => cancel = true,
                        _ => {}
                    }
                }
            });
            if let Some((text, paste)) = chosen {
                if paste {
                    pane.insert(&text);
                } else {
                    ui.ctx().copy_text(text.clone());
                    self.cmds.push(Cmd::Toast(format!("Copiado: {text}")));
                }
                cancel = true;
            }
            if cancel {
                pane.quick = None;
                return;
            }
        }
        let Some(q) = pane.quick.as_ref() else { return };
        let painter = ui.painter_at(term_rect);
        painter.rect_filled(term_rect, 0.0, p.bg.gamma_multiply(0.35));
        let mono = egui::FontId::monospace(ch * 0.72);
        for h in q.hints.iter().filter(|h| h.label.starts_with(&q.typed)) {
            let x0 = term_rect.min.x + h.c0 as f32 * cw;
            let y0 = term_rect.min.y + h.row as f32 * ch;
            let r = Rect::from_min_size(pos2(x0, y0), vec2((h.c1 - h.c0) as f32 * cw, ch));
            painter.rect(r, 2.0, p.yellow.gamma_multiply(0.18), Stroke::new(1.0_f32, p.yellow.gamma_multiply(0.6)), egui::StrokeKind::Inside);
            let rest = &h.label[q.typed.len()..];
            let lw = rest.chars().count() as f32 * cw + 6.0;
            let chip = Rect::from_min_size(pos2(x0 - 2.0, y0 - 1.0), vec2(lw, ch + 2.0));
            painter.rect_filled(chip, 3.0, p.yellow);
            painter.text(chip.center(), egui::Align2::CENTER_CENTER, rest, mono.clone(), Color32::BLACK);
        }
        // Ayuda abajo.
        let help = "Escribe la etiqueta para copiar  ·  Mayúscula la escribe en la terminal  ·  Esc sale";
        let font = egui::FontId::proportional(12.0);
        let w = ui.fonts(|f| f.layout_no_wrap(help.into(), font.clone(), p.text).size().x) + 24.0;
        let r = Rect::from_center_size(pos2(term_rect.center().x, term_rect.max.y - 18.0), vec2(w, 26.0));
        painter.rect(r, 13.0, p.surface, Stroke::new(1.0_f32, p.border), egui::StrokeKind::Inside);
        painter.text(r.center(), egui::Align2::CENTER_CENTER, help, font, p.text);
    }

    fn virtual_selection(
        &mut self,
        ui: &mut egui::Ui,
        pane: &mut Pane,
        resp: &egui::Response,
        term_rect: Rect,
    ) {
        use std::sync::atomic::Ordering;
        let mode = pane.backend.last_content().terminal_mode;
        let alt_mouse = mode.contains(egui_term::TerminalMode::ALT_SCREEN)
            && mode.intersects(egui_term::TerminalMode::MOUSE_MODE);
        if !alt_mouse {
            pane.vsel = None;
            return;
        }
        let (pressed, down, released, shift, pos, wheel) = ui.input(|i| {
            (
                i.pointer.primary_pressed(),
                i.pointer.primary_down(),
                i.pointer.primary_released(),
                i.modifiers.shift,
                i.pointer.latest_pos(),
                i.events
                    .iter()
                    .any(|e| matches!(e, egui::Event::MouseWheel { .. })),
            )
        });
        let local = shift != egui_term::MOUSE_SELECT.load(Ordering::Relaxed);
        let size = pane.backend.last_content().terminal_size;
        let (cw, ch) = (size.cell_width as f32, size.cell_height as f32);
        let rows = pane.screen_rows();
        let (n, cols) = (rows.len(), rows.first().map_or(0, |r| r.len()));
        if n == 0 || cols == 0 || cw <= 0.0 || ch <= 0.0 {
            return;
        }
        let cell_at = |p: egui::Pos2| -> (usize, usize) {
            let c = ((p.x - term_rect.min.x) / cw)
                .floor()
                .clamp(0.0, (cols - 1) as f32);
            let r = ((p.y - term_rect.min.y) / ch)
                .floor()
                .clamp(0.0, (n - 1) as f32);
            (r as usize, c as usize)
        };
        let ctrl = ui.input(|i| i.modifiers.command);
        if pressed && local && resp.hovered() && !ctrl {
            if let Some(p) = pos {
                let (r, c) = cell_at(p);
                pane.vsel = Some(crate::vsel::VSel::new(rows.clone(), r, c));
            }
        } else if pressed {
            pane.vsel = None;
        }
        // Escribir en la app quita la selección (como en cualquier terminal).
        let typed = ui.input(|i| {
            i.events.iter().any(|e| {
                matches!(e, egui::Event::Text(_))
                    || matches!(e, egui::Event::Key { pressed: true, modifiers, key, .. }
                        if !modifiers.ctrl && !modifiers.command
                            && !matches!(key, egui::Key::Escape))
            })
        });
        if typed && pane.id == self.focused && pane.vsel.as_ref().is_some_and(|v| !v.dragging) {
            pane.vsel = None;
        }
        let Some(v) = pane.vsel.as_mut() else { return };
        if wheel {
            v.note_wheel();
        }
        // Arrastrar fuera del borde = scroll automático.
        let mut auto = 0;
        if v.dragging && down {
            if let Some(p) = pos {
                if p.y < term_rect.min.y {
                    auto = 1;
                } else if p.y > term_rect.max.y {
                    auto = -1;
                }
            }
        }
        v.update(rows, pos.map(cell_at));
        // Doble clic = palabra, triple = línea.
        if resp.triple_clicked() {
            v.select_word(true);
        } else if resp.double_clicked() {
            v.select_word(false);
        }
        let finished = (released && v.dragging) || resp.double_clicked() || resp.triple_clicked();
        if released {
            v.dragging = false;
        }
        if finished {
            if v.has_selection() {
                let text = v.text();
                if self.copy_on_select && !text.trim().is_empty() {
                    ui.ctx().copy_text(text);
                    self.cmds.push(Cmd::Toast("Copiado".into()));
                }
            } else {
                pane.vsel = None;
            }
        }
        if auto != 0 {
            let tick = ui.input(|i| (i.time * 12.0) as i64);
            let key = egui::Id::new(("vsel_auto", pane.id));
            let last: i64 = ui.data(|d| d.get_temp(key)).unwrap_or(-1);
            if tick != last {
                ui.data_mut(|d| d.insert_temp(key, tick));
                if let Some(v) = pane.vsel.as_mut() {
                    v.note_wheel();
                }
                pane.scroll_by(auto);
            }
        }
        if pane.vsel.as_ref().is_some_and(|v| v.dragging) {
            ui.ctx().request_repaint();
        }
    }
}

impl egui_tiles::Behavior<Pane> for PaneBehavior<'_> {
    fn pane_ui(&mut self, ui: &mut egui::Ui, _tile_id: TileId, pane: &mut Pane) -> UiResponse {
        let p = pal();
        let rect = ui.max_rect();
        let focused = pane.id == self.focused;
        let footer_h = 34.0;
        let r = CornerRadius::same(theme::RADIUS);
        pane.tick();

        // Panel flotante: fondo propio, borde fino; el activo se enciende en rojo.
        ui.painter()
            .rect_filled(rect, r, p.panel.gamma_multiply(self.opacity));
        let term_rect = Rect::from_min_max(
            rect.min + vec2(16.0, 14.0),
            pos2(rect.max.x - 14.0, rect.max.y - footer_h),
        );
        pane.term_rect = Some(term_rect);
        self.rects.push((pane.id, rect));
        // Con la búsqueda abierta, el teclado va al cuadro de búsqueda.
        let typing_here = focused
            && self.input_enabled
            && !pane.search.open
            && pane.quick.is_none()
            && pane.closing.is_none();
        // La selección propia (Claude Code en pantalla completa) se pinta con
        // los mismos colores que la selección normal de la terminal.
        let highlight = match pane.vsel.as_ref() {
            Some(v) => {
                let rows = pane.screen_rows();
                v.highlight(rows.len(), rows.first().map_or(0, |r| r.len()))
            }
            None => vec![],
        };
        let view = TerminalView::new(ui, &mut pane.backend)
            .set_highlight(highlight)
            .set_focus(typing_here)
            .set_size(term_rect.size())
            .set_theme(self.theme.clone())
            .set_font(self.font.clone());
        let resp = ui.put(term_rect, view);
        if resp.hovered() && ui.input(|i| i.pointer.any_pressed()) && !focused {
            self.cmds.push(Cmd::FocusPane(pane.id));
        }
        let id = pane.id;
        if self.copy_on_select
            && ui.input(|i| i.pointer.primary_released())
            && (resp.drag_stopped() || resp.hovered())
        {
            let sel = pane.backend.selectable_content();
            let sel = sel
                .lines()
                .map(str::trim_end)
                .collect::<Vec<_>>()
                .join("\n");
            if !sel.trim().is_empty() {
                if std::env::var_os("NEON_DEBUG").is_some() {
                    eprintln!("[neon] copiar selección: {sel:?}");
                }
                ui.ctx().copy_text(sel);
            }
        }
        self.virtual_selection(ui, pane, &resp, term_rect);
        // Sugerencia del historial, en gris después del cursor.
        if focused && self.input_enabled {
            if let Some((t, rest)) = pane.ghost.as_ref() {
                if *t == pane.typed && pane.cursor_tail_blank() {
                    if let Some(pos) = pane.cursor_pos() {
                        let painter = ui.painter_at(term_rect);
                        let font = self.font.font_type();
                        let cw = pane.backend.last_content().terminal_size.cell_width as f32;
                        for (i, ch) in rest.chars().enumerate() {
                            let x = pos.x + i as f32 * cw + cw / 2.0;
                            if x > term_rect.max.x {
                                break;
                            }
                            painter.text(
                                egui::pos2(x, pos.y),
                                egui::Align2::CENTER_TOP,
                                ch,
                                font.clone(),
                                p.muted.gamma_multiply(0.8),
                            );
                        }
                    }
                }
            }
        }
        if self.blocks && pane.quick.is_none() {
            self.blocks_ui(ui, pane, term_rect);
        }
        self.ai_ui(ui, pane, term_rect);
        if pane.changes.is_some() {
            self.changes_ui(ui, pane, term_rect);
        }
        if pane.quick.is_some() {
            self.quick_ui(ui, pane, term_rect, focused);
        }
        if self.middle_paste
            && resp.hovered()
            && ui.input(|i| i.pointer.button_clicked(egui::PointerButton::Middle))
        {
            self.cmds.push(Cmd::FocusPane(id));
            self.cmds.push(Cmd::Paste(id));
        }
        let has_selection = !pane.backend.selectable_content().trim().is_empty();
        resp.context_menu(|ui| {
            ui.set_min_width(220.0);
            let item = |ui: &mut egui::Ui, label: &str, keys: &str| {
                ui.add(egui::Button::new(label).shortcut_text(keys))
                    .clicked()
            };
            if ui
                .add_enabled(
                    has_selection,
                    egui::Button::new("Copiar").shortcut_text("Ctrl+Shift+C"),
                )
                .clicked()
            {
                self.cmds.push(Cmd::CopySelection(id));
                ui.close_menu();
            }
            if item(ui, "Pegar", "Ctrl+Shift+V") {
                self.cmds.push(Cmd::Paste(id));
                ui.close_menu();
            }
            if item(ui, "Copiar salida del último comando", "") {
                self.cmds.push(Cmd::FocusPane(id));
                self.cmds.push(Cmd::CopyLastOutput);
                ui.close_menu();
            }
            ui.separator();
            if item(ui, "Buscar…", "") {
                self.cmds.push(Cmd::FocusPane(id));
                self.cmds.push(Cmd::Search);
                ui.close_menu();
            }
            if item(ui, "Limpiar", "") {
                self.cmds.push(Cmd::ClearPane(id));
                ui.close_menu();
            }
            ui.separator();
            if item(ui, "Dividir a la derecha", "") {
                self.cmds.push(Cmd::FocusPane(id));
                self.cmds.push(Cmd::SplitRight);
                ui.close_menu();
            }
            if item(ui, "Dividir hacia abajo", "") {
                self.cmds.push(Cmd::FocusPane(id));
                self.cmds.push(Cmd::SplitDown);
                ui.close_menu();
            }
            if item(ui, "Maximizar / restaurar panel", "") {
                self.cmds.push(Cmd::FocusPane(id));
                self.cmds.push(Cmd::ToggleZoom);
                ui.close_menu();
            }
            ui.separator();
            if item(ui, "Cerrar panel", "") {
                self.cmds.push(Cmd::FocusPane(id));
                self.cmds.push(Cmd::ClosePane);
                ui.close_menu();
            }
        });

        search_highlights(ui, pane, term_rect);
        scrollbar(ui, pane, term_rect, resp.hovered());
        if pane.search.open {
            search_bar(ui, pane, term_rect, focused && self.input_enabled);
        }

        let anim = egui_term::ANIMATIONS.load(std::sync::atomic::Ordering::Relaxed);
        let k = if anim {
            ui.ctx()
                .animate_bool_with_time(egui::Id::new(("focus", pane.id)), focused, 0.16)
        } else if focused {
            1.0
        } else {
            0.0
        };
        let lerp = |a: Color32, b: Color32| a.lerp_to_gamma(b, k);
        let border = Stroke::new(1.0_f32, lerp(p.border, p.red.gamma_multiply(0.45)));
        if k > 0.0 {
            ui.painter().rect_stroke(
                rect.expand(1.5),
                r,
                Stroke::new(1.5_f32, p.red.gamma_multiply(0.14 * k)),
                StrokeKind::Outside,
            );
        }
        ui.painter()
            .rect_stroke(rect.shrink(0.5), r, border, StrokeKind::Inside);

        let footer_rect = Rect::from_min_max(pos2(rect.min.x, rect.max.y - footer_h), rect.max);
        let mut fui = ui.new_child(
            UiBuilder::new()
                .max_rect(footer_rect.shrink2(vec2(12.0, 0.0)))
                .layout(Layout::left_to_right(Align::Center)),
        );
        footer(&mut fui, pane, &mut self.cmds);

        // Aparición: el panel nuevo y la pestaña recién elegida entran con un fundido.
        if anim {
            let born = (pane.born.elapsed().as_secs_f32() / 0.22).min(1.0);
            let t = born.min(self.switch);
            if t < 1.0 {
                let ease = 1.0 - (1.0 - t).powi(3);
                ui.painter()
                    .rect_filled(rect, r, p.bg.gamma_multiply(1.0 - ease));
                ui.ctx().request_repaint();
            }
        }
        // Cierre: se encoge un poco y se funde con el fondo.
        if let Some(t0) = pane.closing {
            let t = (t0.elapsed().as_secs_f32() / CLOSE_ANIM.as_secs_f32()).min(1.0);
            let ease = t * t;
            ui.painter().rect_filled(rect, r, p.bg.gamma_multiply(ease));
            let inner = rect.shrink(rect.width().min(rect.height()) * 0.04 * ease);
            ui.painter().rect_stroke(
                inner,
                r,
                Stroke::new(1.0_f32, p.border.gamma_multiply(1.0 - ease)),
                StrokeKind::Inside,
            );
            ui.ctx().request_repaint();
        }
        UiResponse::None
    }

    fn tab_title_for_pane(&mut self, pane: &Pane) -> egui::WidgetText {
        pane.display_title().into()
    }

    fn gap_width(&self, _style: &egui::Style) -> f32 {
        10.0
    }

    fn resize_stroke(&self, _style: &egui::Style, state: egui_tiles::ResizeState) -> Stroke {
        match state {
            egui_tiles::ResizeState::Idle => Stroke::NONE,
            _ => Stroke::new(1.0_f32, pal().red.gamma_multiply(0.7)),
        }
    }
}

/// Celda (fila, columna) visible → rectángulo en pantalla.
fn cell_rect(pane: &Pane, term_rect: Rect, row: i64, c0: usize, c1: usize) -> Rect {
    let size = &pane.backend.last_content().terminal_size;
    let (cw, ch) = (size.cell_width as f32, size.cell_height as f32);
    Rect::from_min_size(
        pos2(
            term_rect.min.x + c0 as f32 * cw,
            term_rect.min.y + row as f32 * ch,
        ),
        vec2((c1 - c0) as f32 * cw, ch),
    )
}

/// Resalta los resultados de la búsqueda que están a la vista.
fn search_highlights(ui: &egui::Ui, pane: &Pane, term_rect: Rect) {
    if !pane.search.open || pane.search.matches.is_empty() {
        return;
    }
    let p = pal();
    let (hist, screen, offset) = pane.scroll_info();
    let top = hist as i64 - offset as i64;
    for (i, &(line, c0, c1)) in pane.search.matches.iter().enumerate() {
        let row = line - top;
        if row < 0 || row >= screen as i64 {
            continue;
        }
        let r = cell_rect(pane, term_rect, row, c0, c1).expand(1.0);
        let current = i == pane.search.current;
        let color = if current { p.orange } else { p.yellow };
        ui.painter().rect_filled(
            r,
            CornerRadius::same(2),
            color.gamma_multiply(if current { 0.45 } else { 0.22 }),
        );
        if current {
            ui.painter().rect_stroke(
                r,
                CornerRadius::same(2),
                Stroke::new(1.5_f32, p.orange),
                StrokeKind::Outside,
            );
        }
    }
}

/// Barra de scroll fina: aparece al pasar el mouse por el borde derecho o al
/// moverse por el historial, y se puede arrastrar.
fn scrollbar(ui: &mut egui::Ui, pane: &mut Pane, term_rect: Rect, hovered: bool) {
    let (hist, screen, offset) = pane.scroll_info();
    if hist == 0 {
        return;
    }
    let p = pal();
    let track = Rect::from_min_max(
        pos2(term_rect.max.x + 3.0, term_rect.min.y),
        pos2(term_rect.max.x + 9.0, term_rect.max.y),
    );
    let near = ui
        .input(|i| i.pointer.hover_pos())
        .is_some_and(|pos| track.expand2(vec2(8.0, 0.0)).contains(pos));
    let since = pane.scroll_seen.1.elapsed().as_secs_f32();
    let id = ui.id().with(("scrollbar", pane.id));
    let dragging = ui.ctx().is_being_dragged(id);
    let alpha = if near || dragging {
        1.0
    } else {
        (1.0 - (since - 0.9) / 0.4).clamp(0.0, 1.0)
    };
    if alpha <= 0.0 {
        let _ = hovered;
        return;
    }
    if alpha < 1.0 {
        ui.ctx().request_repaint();
    }
    let total = (hist + screen) as f32;
    let h = track.height();
    let thumb_h = (h * screen as f32 / total).max(24.0);
    let top_frac = (hist - offset) as f32 / hist as f32;
    let y = track.min.y + top_frac * (h - thumb_h);
    let thumb = Rect::from_min_size(pos2(track.min.x, y), vec2(track.width(), thumb_h));
    let resp = ui.interact(track.expand2(vec2(4.0, 0.0)), id, Sense::drag());
    if resp.dragged() {
        let dy = resp.drag_delta().y;
        let lines = (dy / (h - thumb_h).max(1.0) * hist as f32).round() as i32;
        // Arrastrar hacia abajo = ir hacia lo más nuevo.
        pane.scroll_by(-lines);
    }
    let color = if near || dragging { p.muted } else { p.border };
    ui.painter()
        .rect_filled(thumb, CornerRadius::same(3), color.gamma_multiply(alpha));
}

/// Cuadro de búsqueda arriba a la derecha del panel.
fn search_bar(ui: &mut egui::Ui, pane: &mut Pane, term_rect: Rect, active: bool) {
    let p = pal();
    let w = 330.0_f32.min(term_rect.width() - 8.0);
    let area = Rect::from_min_size(
        pos2(term_rect.max.x - w, term_rect.min.y - 6.0),
        vec2(w, 34.0),
    );
    ui.painter()
        .rect_filled(area, CornerRadius::same(8), p.surface);
    ui.painter().rect_stroke(
        area,
        CornerRadius::same(8),
        Stroke::new(1.0_f32, p.border),
        StrokeKind::Inside,
    );
    let mut close = false;
    // Some(true) = ir hacia arriba (más antiguo). Se empieza por el resultado
    // más reciente, así que Enter sube, como en Windows Terminal.
    let mut go_up: Option<bool> = None;
    let mut child = ui.new_child(
        UiBuilder::new()
            .max_rect(area.shrink2(vec2(10.0, 4.0)))
            .layout(Layout::left_to_right(Align::Center)),
    );
    child.spacing_mut().item_spacing.x = 4.0;
    let n = pane.search.matches.len();
    let count = if pane.search.query.is_empty() {
        String::new()
    } else if n == 0 {
        "0".into()
    } else {
        format!("{}/{}", pane.search.current + 1, n)
    };
    let edit = child.add(
        egui::TextEdit::singleline(&mut pane.search.query)
            .hint_text("Buscar")
            .frame(false)
            .desired_width(w - 130.0)
            .font(FontId::proportional(13.0)),
    );
    if pane.search.focus && active {
        edit.request_focus();
        pane.search.focus = false;
    }
    if edit.lost_focus() && child.input(|i| i.key_pressed(egui::Key::Enter)) {
        go_up = Some(!child.input(|i| i.modifiers.shift));
        edit.request_focus();
    }
    // Esc le quita el foco al cuadro (egui) antes de que lo veamos: se revisan ambos casos.
    if (edit.has_focus() || edit.lost_focus()) && child.input(|i| i.key_pressed(egui::Key::Escape))
    {
        close = true;
    }
    let color = if n == 0 && !pane.search.query.is_empty() {
        p.red
    } else {
        p.muted
    };
    child.label(RichText::new(count).size(12.0).color(color));
    let small = |ui: &mut egui::Ui, icon: Icon, tip: &str| {
        let (r, resp) = ui.allocate_exact_size(vec2(22.0, 22.0), Sense::click());
        if resp.hovered() {
            ui.painter().rect_filled(r, CornerRadius::same(5), p.hover);
        }
        paint_icon(ui.painter(), icon, r.center(), p.muted);
        resp.on_hover_text(tip).clicked()
    };
    if small(&mut child, Icon::Up, "Más arriba (Enter)") {
        go_up = Some(true);
    }
    if small(&mut child, Icon::Down, "Más abajo (Shift+Enter)") {
        go_up = Some(false);
    }
    if small(&mut child, Icon::Cross, "Cerrar (Esc)") {
        close = true;
    }
    if let Some(up) = go_up {
        pane.search_step(!up);
    }
    if close {
        pane.close_search();
    }
}

/// Barra inferior de cada panel: cápsulas con rama, cambios y último comando.
pub fn footer(ui: &mut egui::Ui, pane: &Pane, actions: &mut Vec<Cmd>) {
    ui.spacing_mut().item_spacing.x = 6.0;
    let wide = ui.available_width() > 560.0;
    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
        // Mientras corre un comando: pedir aviso cuando termine.
        if pane.running().is_some() {
            let (text, color, hint) = if pane.notify_done {
                ("te aviso ✓", pal().green, "Te llega una notificación al terminar · clic: cancelar")
            } else {
                ("avisar al terminar", pal().muted, "Recibir una notificación cuando termine este comando")
            };
            if chip(ui, text, color)
                .on_hover_text(hint)
                .interact(Sense::click())
                .on_hover_cursor(egui::CursorIcon::PointingHand)
                .clicked()
            {
                actions.push(Cmd::NotifyWhenDone(pane.id));
            }
        }
        if let Some(last) = &pane.last {
            let r = if last.ok {
                chip_icon(
                    ui,
                    Icon::Check,
                    fmt_ms(last.duration_ms).trim(),
                    pal().green,
                )
            } else {
                chip_icon(ui, Icon::Cross, "falló", pal().red)
            };
            r.on_hover_text(&last.command);
        }
        ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
            ui.set_clip_rect(ui.max_rect().intersect(ui.clip_rect()));
            let Some(g) = &pane.git.status else {
                chip(ui, &short_path(&pane.cwd), pal().muted);
                return;
            };
            let branch = chip_icon(ui, Icon::Branch, &g.branch, pal().violet)
                .on_hover_text("Cambiar de rama / ver commits");
            egui::popup::popup_above_or_below_widget(
                ui,
                ui.make_persistent_id(("branch_menu", pane.id)),
                &branch,
                egui::AboveOrBelow::Above,
                egui::PopupCloseBehavior::CloseOnClick,
                |ui| {
                    ui.set_min_width(280.0);
                    ui.label(RichText::new("Ramas").size(11.5).color(pal().muted));
                    for b in &g.branches {
                        let current = *b == g.branch;
                        let text = RichText::new(b).monospace().color(if current {
                            pal().violet
                        } else {
                            pal().text
                        });
                        if ui.add_enabled(!current, egui::Button::new(text)).clicked() {
                            actions.push(Cmd::Insert(pane.id, format!("git switch {b}")));
                        }
                    }
                    ui.add_space(4.0);
                    ui.label(
                        RichText::new("Commits recientes")
                            .size(11.5)
                            .color(pal().muted),
                    );
                    for c in &g.commits {
                        let mut job = egui::text::LayoutJob::default();
                        let f = FontId::monospace(12.5);
                        job.append(
                            &c.hash,
                            0.0,
                            egui::TextFormat::simple(f.clone(), pal().yellow),
                        );
                        job.append(
                            &format!("  {}", truncate(&c.subject, 36)),
                            0.0,
                            egui::TextFormat::simple(f, pal().text),
                        );
                        if ui
                            .add(egui::Button::new(job))
                            .on_hover_text(ago(now(), c.ts))
                            .clicked()
                        {
                            actions.push(Cmd::Insert(pane.id, format!("git show {}", c.hash)));
                        }
                    }
                },
            );
            if branch.clicked() {
                ui.memory_mut(|m| m.toggle_popup(ui.make_persistent_id(("branch_menu", pane.id))));
            }

            if g.ahead > 0 {
                chip_icon(ui, Icon::Up, &g.ahead.to_string(), pal().yellow)
                    .on_hover_text("Commits sin subir (git push)");
            }
            if g.behind > 0 {
                chip_icon(ui, Icon::Down, &g.behind.to_string(), pal().orange)
                    .on_hover_text("Commits por bajar (git pull)");
            }
            if g.is_clean() {
                chip(ui, "limpio", pal().green);
            } else {
                if g.staged > 0 {
                    chip(ui, &format!("+{}", g.staged), pal().green).on_hover_text("En stage");
                }
                let mut open_changes = false;
                if g.modified > 0 {
                    open_changes |= chip(ui, &format!("~{}", g.modified), pal().orange)
                        .on_hover_text("Modificados · clic: ver cambios")
                        .interact(Sense::click())
                        .clicked();
                }
                if g.untracked > 0 {
                    open_changes |= chip(ui, &format!("?{}", g.untracked), pal().cyan)
                        .on_hover_text("Sin seguimiento · clic: ver cambios")
                        .interact(Sense::click())
                        .clicked();
                }
                if open_changes {
                    actions.push(Cmd::FocusPane(pane.id));
                    actions.push(Cmd::ClaudeChanges);
                }
                if g.conflicts > 0 {
                    chip(ui, &format!("!{}", g.conflicts), pal().red).on_hover_text("Conflictos");
                }
            }
            if let (true, Some(c)) = (wide, g.commits.first()) {
                ui.add_space(4.0);
                ui.label(
                    RichText::new(&c.hash)
                        .size(12.0)
                        .color(pal().muted)
                        .monospace(),
                );
                ui.label(
                    RichText::new(truncate(&c.subject, 40))
                        .size(12.0)
                        .color(pal().muted),
                )
                .on_hover_text(ago(now(), c.ts));
            }
        });
    });
}

// ─────────────────────────── Helpers UI ───────────────────────────

pub const WINDOW_BUTTON_W: f32 = 46.0;

#[derive(Clone, Copy, PartialEq)]
pub enum WindowButton {
    Minimize,
    Maximize,
    Restore,
    Close,
}

/// Botones de ventana dibujados a mano, con las medidas de Windows 11.
/// Cerrar se pone rojo al pasar el mouse.
pub fn window_button(ui: &mut egui::Ui, kind: WindowButton, h: f32) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(WINDOW_BUTTON_W, h), Sense::click());
    let hovered = resp.hovered();
    if hovered {
        let fill = if kind == WindowButton::Close {
            Color32::from_rgb(0xe8, 0x11, 0x23)
        } else {
            pal().hover
        };
        ui.painter().rect_filled(rect, 0.0, fill);
    }
    let color = if hovered { pal().text } else { pal().muted };
    let stroke = Stroke::new(1.0_f32, color);
    let c = rect.center();
    let p = ui.painter();
    match kind {
        WindowButton::Minimize => {
            p.hline((c.x - 5.0)..=(c.x + 5.0), c.y, stroke);
        }
        WindowButton::Maximize => {
            p.rect_stroke(
                Rect::from_center_size(c, vec2(10.0, 10.0)),
                CornerRadius::same(1),
                stroke,
                StrokeKind::Inside,
            );
        }
        WindowButton::Restore => {
            p.rect_stroke(
                Rect::from_center_size(c + vec2(-1.0, 1.0), vec2(8.0, 8.0)),
                CornerRadius::same(1),
                stroke,
                StrokeKind::Inside,
            );
            p.line_segment([c + vec2(-2.0, -4.0), c + vec2(4.0, -4.0)], stroke);
            p.line_segment([c + vec2(4.0, -4.0), c + vec2(4.0, 2.0)], stroke);
        }
        WindowButton::Close => {
            p.line_segment([c + vec2(-5.0, -5.0), c + vec2(5.0, 5.0)], stroke);
            p.line_segment([c + vec2(-5.0, 5.0), c + vec2(5.0, -5.0)], stroke);
        }
    }
    let hint = match kind {
        WindowButton::Minimize => "Minimizar",
        WindowButton::Maximize => "Maximizar",
        WindowButton::Restore => "Restaurar",
        WindowButton::Close => "Cerrar",
    };
    resp.on_hover_text(hint)
}

/// Sin barra de título nativa hay que permitir redimensionar desde los bordes.
pub fn resize_edges(ctx: &egui::Context) {
    use egui::{CursorIcon, ResizeDirection as D};
    if ctx.input(|i| i.viewport().maximized.unwrap_or(false)) {
        return;
    }
    let Some(pos) = ctx.input(|i| i.pointer.hover_pos()) else {
        return;
    };
    let r = ctx.screen_rect();
    let m = 5.0;
    let (l, rt, t, b) = (
        pos.x - r.min.x < m,
        r.max.x - pos.x < m,
        pos.y - r.min.y < m,
        r.max.y - pos.y < m,
    );
    let dir = match (l, rt, t, b) {
        (true, _, true, _) => D::NorthWest,
        (_, true, true, _) => D::NorthEast,
        (true, _, _, true) => D::SouthWest,
        (_, true, _, true) => D::SouthEast,
        (true, ..) => D::West,
        (_, true, ..) => D::East,
        (_, _, true, _) => D::North,
        (.., true) => D::South,
        _ => return,
    };
    let icon = match dir {
        D::North | D::South => CursorIcon::ResizeVertical,
        D::East | D::West => CursorIcon::ResizeHorizontal,
        D::NorthWest | D::SouthEast => CursorIcon::ResizeNwSe,
        D::NorthEast | D::SouthWest => CursorIcon::ResizeNeSw,
    };
    ctx.set_cursor_icon(icon);
    if ctx.input(|i| i.pointer.primary_pressed()) {
        ctx.send_viewport_cmd(egui::ViewportCommand::BeginResize(dir));
    }
}

pub fn dot(ui: &mut egui::Ui, c: Color32) {
    let (rect, _) = ui.allocate_exact_size(vec2(10.0, 14.0), Sense::hover());
    ui.painter().circle_filled(rect.center(), 3.5, c);
}

/// Cápsula de color: texto del color sobre un fondo teñido del mismo color.
pub fn chip(ui: &mut egui::Ui, text: &str, color: Color32) -> egui::Response {
    let galley = ui
        .painter()
        .layout_no_wrap(text.to_string(), FontId::proportional(12.0), color);
    let size = vec2(galley.size().x + 16.0, 20.0);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    let fill = if resp.hovered() {
        theme::mix(pal().panel, color, 0.24)
    } else {
        theme::tint(color)
    };
    ui.painter().rect_filled(rect, CornerRadius::same(6), fill);
    ui.painter()
        .galley(rect.center() - galley.size() / 2.0, galley, color);
    resp
}

/// Íconos dibujados a mano: no dependen de que la fuente tenga el símbolo
/// (en Windows, Segoe UI no trae ⎇ ni ▾ y salían como cuadrados).
#[derive(Clone, Copy, PartialEq)]
pub enum Icon {
    Branch,
    Check,
    Cross,
    Up,
    Down,
}

pub fn paint_icon(p: &egui::Painter, icon: Icon, c: egui::Pos2, color: Color32) {
    let st = Stroke::new(1.4_f32, color);
    match icon {
        Icon::Branch => {
            // Dos commits en una línea y una rama que sale hacia arriba a la derecha.
            let (a, b, r) = (
                c + vec2(-3.0, -4.5),
                c + vec2(-3.0, 4.5),
                c + vec2(3.5, -3.0),
            );
            p.line_segment([a, b], st);
            p.add(egui::Shape::CubicBezier(
                egui::epaint::CubicBezierShape::from_points_stroke(
                    [
                        r,
                        r + vec2(0.0, 4.0),
                        c + vec2(-3.0, 0.5),
                        c + vec2(-3.0, 3.0),
                    ],
                    false,
                    Color32::TRANSPARENT,
                    st,
                ),
            ));
            for q in [a, b, r] {
                p.circle_filled(q, 1.9, color);
            }
        }
        Icon::Check => {
            p.line_segment([c + vec2(-4.0, 0.0), c + vec2(-1.2, 3.0)], st);
            p.line_segment([c + vec2(-1.2, 3.0), c + vec2(4.0, -3.5)], st);
        }
        Icon::Cross => {
            p.line_segment([c + vec2(-3.5, -3.5), c + vec2(3.5, 3.5)], st);
            p.line_segment([c + vec2(-3.5, 3.5), c + vec2(3.5, -3.5)], st);
        }
        Icon::Up | Icon::Down => {
            let d = if icon == Icon::Up { -1.0 } else { 1.0 };
            p.line_segment([c + vec2(0.0, -4.0 * d), c + vec2(0.0, 4.0 * d)], st);
            p.line_segment([c + vec2(0.0, 4.0 * d), c + vec2(-3.0, 1.0 * d)], st);
            p.line_segment([c + vec2(0.0, 4.0 * d), c + vec2(3.0, 1.0 * d)], st);
        }
    }
}

/// Cápsula con ícono a la izquierda. La de rama lleva además una flechita de menú.
pub fn chip_icon(ui: &mut egui::Ui, icon: Icon, text: &str, color: Color32) -> egui::Response {
    let galley = ui
        .painter()
        .layout_no_wrap(text.to_string(), FontId::proportional(12.0), color);
    let chevron = icon == Icon::Branch;
    let icon_w = 16.0;
    let extra = if chevron { 14.0 } else { 0.0 };
    let text_w = if text.is_empty() {
        0.0
    } else {
        galley.size().x + 4.0
    };
    let size = vec2(8.0 + icon_w + text_w + extra + 6.0, 20.0);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    let fill = if resp.hovered() {
        theme::mix(pal().panel, color, 0.24)
    } else {
        theme::tint(color)
    };
    let p = ui.painter();
    p.rect_filled(rect, CornerRadius::same(6), fill);
    paint_icon(
        p,
        icon,
        pos2(rect.min.x + 8.0 + icon_w / 2.0, rect.center().y),
        color,
    );
    let tx = rect.min.x + 8.0 + icon_w + 2.0;
    p.galley(
        pos2(tx, rect.center().y - galley.size().y / 2.0),
        galley,
        color,
    );
    if chevron {
        let c = pos2(rect.max.x - 11.0, rect.center().y + 1.0);
        let st = Stroke::new(1.3_f32, color);
        p.line_segment([c + vec2(-3.0, -1.5), c + vec2(0.0, 1.5)], st);
        p.line_segment([c + vec2(0.0, 1.5), c + vec2(3.0, -1.5)], st);
    }
    resp
}

pub fn popup_frame() -> Frame {
    Frame::new()
        .fill(pal().surface)
        .stroke(Stroke::new(1.0_f32, pal().border))
        .corner_radius(CornerRadius::same(theme::RADIUS))
        .inner_margin(Margin::same(8))
}

/// Fila del historial con el comando coloreado. Devuelve true si se hizo click.
pub fn history_row(
    ui: &mut egui::Ui,
    e: &Entry,
    selected: bool,
    t: i64,
    h: f32,
    compact: bool,
) -> bool {
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), h), Sense::click());
    let p = ui.painter();
    if selected {
        p.rect_filled(
            rect,
            CornerRadius::same(6),
            theme::mix(pal().surface, pal().red, 0.16),
        );
    } else if resp.hovered() {
        p.rect_filled(rect, CornerRadius::same(6), pal().hover);
    }
    let mark = if !e.last_ok {
        pal().red
    } else if e.here_count > 0 {
        pal().green
    } else {
        pal().border
    };
    p.circle_filled(pos2(rect.min.x + 12.0, rect.center().y), 3.0, mark);
    let cmd = truncate(&e.command, if compact { 64 } else { 72 });
    let galley =
        ui.fonts(|f| f.layout_job(theme::highlight_command(&cmd, FontId::monospace(13.0))));
    p.galley(
        pos2(rect.min.x + 24.0, rect.center().y - galley.size().y / 2.0),
        galley,
        pal().text,
    );
    let meta = if compact {
        ago(t, e.last_ts)
    } else {
        format!("×{}  ·  {}", e.count, ago(t, e.last_ts))
    };
    p.text(
        pos2(rect.max.x - 10.0, rect.center().y),
        Align2::RIGHT_CENTER,
        meta,
        FontId::proportional(11.5),
        pal().muted,
    );
    resp.clicked()
}

pub fn truncate(s: &str, max: usize) -> String {
    let line = s.lines().next().unwrap_or("");
    if line.chars().count() <= max && !s.contains('\n') {
        line.to_string()
    } else {
        format!(
            "{}…",
            line.chars().take(max.saturating_sub(1)).collect::<String>()
        )
    }
}

pub fn short_path(p: &str) -> String {
    if let Some(home) = dirs::home_dir() {
        let h = home.to_string_lossy();
        if let Some(rest) = p.strip_prefix(h.as_ref()) {
            return format!("~{rest}");
        }
    }
    p.to_string()
}

pub fn fmt_ms(ms: Option<i64>) -> String {
    match ms {
        Some(ms) if ms >= 60_000 => format!("{}m {}s", ms / 60_000, (ms % 60_000) / 1000),
        Some(ms) if ms >= 1000 => format!("{:.1}s", ms as f64 / 1000.0),
        Some(ms) => format!("{ms}ms"),
        None => String::new(),
    }
}

pub fn ago(now: i64, ts: i64) -> String {
    let d = (now - ts).max(0);
    match d {
        0..60 => "recién".into(),
        60..3600 => format!("hace {} min", d / 60),
        3600..86400 => format!("hace {} h", d / 3600),
        _ => format!("hace {} d", d / 86400),
    }
}
