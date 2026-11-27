//! Ayuda: recorrido de bienvenida (primer arranque) y pantalla de atajos (F1).

use egui::{Align, CornerRadius, Frame, Key, Layout, Margin, Modifiers, RichText, Stroke, vec2};

use crate::commands::{Cmd, binding_for, bindables};
use crate::theme::pal;

/// Versión del recorrido: si se agregan tarjetas nuevas, se sube y se vuelve
/// a mostrar una vez.
pub const TOUR_VERSION: u32 = 1;

pub enum Help {
    Tour(usize),
    Shortcuts(String),
}

struct Card {
    title: &'static str,
    lines: &'static [(&'static str, &'static str)],
}

const CARDS: &[Card] = &[
    Card {
        title: "Bienvenido a Neon",
        lines: &[
            ("Ctrl+Shift+P", "La paleta: todo lo que Neon sabe hacer, buscando por nombre."),
            ("F1", "Todos los atajos de teclado."),
            ("Ctrl+Alt+T", "Desde cualquier app: mostrar u ocultar Neon."),
        ],
    },
    Card {
        title: "Escribir menos",
        lines: &[
            ("cd ", "Muestra las carpetas: escribe para filtrar, Tab entra, Enter va."),
            ("→", "Acepta la sugerencia en gris que sale de tu historial."),
            ("↑", "Historial de esta carpeta pegado al cursor (Ctrl+R busca en todo)."),
            ("# pedido", "Claude escribe el comando (por ejemplo «# borra las ramas mergeadas»)."),
        ],
    },
    Card {
        title: "Cada comando es un bloque",
        lines: &[
            ("Barra izquierda", "Verde si salió bien, rojo si falló, amarillo mientras corre."),
            ("Pasar el mouse", "Copiar el comando o su salida; si falló, ✳ Explicar el error."),
            ("Ctrl+↑ / ↓", "Saltar entre comandos. Al hacer scroll, el comando queda arriba."),
        ],
    },
    Card {
        title: "Herramientas a mano",
        lines: &[
            ("Ctrl+Shift+A", "Buscar un archivo del proyecto y abrirlo o pegar su ruta."),
            ("Ctrl+Shift+Space", "Etiquetas sobre URLs, rutas y hashes: una letra los copia."),
            ("Ctrl+Shift+H", "Lo último que copiaste, para pegarlo de nuevo."),
            ("Ctrl+clic", "Abre links y rutas (src/app.rs:120 abre en esa línea)."),
        ],
    },
    Card {
        title: "Hecho para Claude Code",
        lines: &[
            ("Ctrl+Shift+K", "Abre Claude Code en un panel al lado, en la misma carpeta."),
            ("Ctrl+Shift+G", "Qué archivos cambió Claude, con su diff."),
            ("Pestañas", "Arco naranja: Claude trabaja · ✓ terminó · punto rojo: te necesita."),
            ("Arrastrar", "Selecciona y copia dentro de Claude Code, aunque hagas scroll."),
        ],
    },
];

/// Atajos que no se configuran pero conviene conocer.
const FIXED: &[(&str, &str)] = &[
    ("cd  + Tab / Enter", "Elegir carpeta"),
    ("→", "Aceptar la sugerencia del historial"),
    ("# pedido + Enter", "Claude escribe el comando"),
    ("Ctrl+clic", "Abrir link o ruta"),
    ("Shift+Enter", "Nueva línea en Claude Code"),
    ("Clic medio", "Pegar"),
    ("Esc", "Cerrar ventanas y listas"),
];

/// Dibuja la ayuda abierta. Devuelve false cuando se cierra.
pub fn show(ctx: &egui::Context, help: &mut Help, quake_key: &str, cmds: &mut Vec<Cmd>) -> bool {
    match help {
        Help::Tour(step) => tour(ctx, step, cmds),
        Help::Shortcuts(q) => shortcuts(ctx, q, quake_key),
    }
}

fn frame() -> Frame {
    let p = pal();
    Frame::new()
        .fill(p.surface)
        .stroke(Stroke::new(1.0_f32, p.border))
        .corner_radius(CornerRadius::same(14))
        .inner_margin(Margin::same(22))
}

fn key_chip(ui: &mut egui::Ui, k: &str) {
    let p = pal();
    Frame::new()
        .fill(p.bg)
        .stroke(Stroke::new(1.0_f32, p.border))
        .corner_radius(CornerRadius::same(6))
        .inner_margin(Margin::symmetric(8, 3))
        .show(ui, |ui| {
            ui.label(RichText::new(k).monospace().size(12.5).color(p.text));
        });
}

fn tour(ctx: &egui::Context, step: &mut usize, cmds: &mut Vec<Cmd>) -> bool {
    let p = pal();
    let n = CARDS.len();
    let (left, right, esc) = ctx.input_mut(|i| {
        (
            i.consume_key(Modifiers::NONE, Key::ArrowLeft),
            i.consume_key(Modifiers::NONE, Key::ArrowRight) || i.consume_key(Modifiers::NONE, Key::Enter),
            i.consume_key(Modifiers::NONE, Key::Escape),
        )
    });
    let mut open = !esc;
    if left {
        *step = step.saturating_sub(1);
    }
    if right {
        if *step + 1 >= n {
            open = false;
        } else {
            *step += 1;
        }
    }
    let card = &CARDS[(*step).min(n - 1)];
    let modal = egui::Modal::new(egui::Id::new("tour")).frame(frame()).show(ctx, |ui| {
        ui.set_width(540.0);
        ui.horizontal(|ui| {
            ui.label(RichText::new(card.title).size(20.0).strong());
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.label(RichText::new(format!("{} / {n}", *step + 1)).color(p.muted));
            });
        });
        ui.add_space(14.0);
        for (k, what) in card.lines {
            ui.horizontal(|ui| {
                // Columna fija para que los textos queden alineados.
                ui.allocate_ui_with_layout(vec2(150.0, 26.0), Layout::left_to_right(Align::Center), |ui| {
                    ui.set_min_width(150.0);
                    key_chip(ui, k);
                });
                ui.label(RichText::new(*what).color(p.text).size(13.5));
            });
            ui.add_space(6.0);
        }
        ui.add_space(14.0);
        ui.horizontal(|ui| {
            // Puntitos de progreso.
            for i in 0..n {
                let (r, _) = ui.allocate_exact_size(vec2(14.0, 10.0), egui::Sense::hover());
                let c = if i == *step { p.red } else { p.border };
                ui.painter().circle_filled(r.center(), 3.5, c);
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let last = *step + 1 >= n;
                let next = egui::Button::new(
                    RichText::new(if last { "Empezar" } else { "Siguiente" }).color(egui::Color32::WHITE),
                )
                .fill(p.red);
                if ui.add(next).clicked() {
                    if last {
                        open = false;
                    } else {
                        *step += 1;
                    }
                }
                if *step > 0 && ui.button("Atrás").clicked() {
                    *step -= 1;
                }
                if !last && ui.button("Saltar").clicked() {
                    open = false;
                }
                if last && ui.button("Ver todos los atajos").clicked() {
                    cmds.push(Cmd::ShowShortcuts);
                    open = false;
                }
            });
        });
    });
    if modal.should_close() {
        open = false;
    }
    open
}

fn shortcuts(ctx: &egui::Context, query: &mut String, quake_key: &str) -> bool {
    let p = pal();
    let mut open = !ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape));
    let settings = crate::settings::get();
    let mut rows: Vec<(String, String)> = bindables()
        .iter()
        .filter_map(|b| {
            binding_for(b, &settings)
                .map(|sc| (b.label.to_string(), crate::settings::format_shortcut(&sc)))
        })
        .collect();
    if settings.quake {
        rows.push(("Mostrar u ocultar Neon (desde cualquier app)".into(), quake_key.to_string()));
    }
    rows.extend(FIXED.iter().map(|(k, l)| (l.to_string(), k.to_string())));
    let q = query.to_lowercase();
    rows.retain(|(l, k)| q.is_empty() || l.to_lowercase().contains(&q) || k.to_lowercase().contains(&q));
    let modal = egui::Modal::new(egui::Id::new("shortcuts")).frame(frame()).show(ctx, |ui| {
        ui.set_width(600.0);
        ui.horizontal(|ui| {
            ui.label(RichText::new("Atajos de teclado").size(18.0).strong());
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui.button("Cerrar").clicked() {
                    open = false;
                }
            });
        });
        ui.add_space(8.0);
        let edit = ui.add(
            egui::TextEdit::singleline(query)
                .hint_text("Buscar (por ejemplo «pegar» o «Ctrl+Shift»)")
                .desired_width(f32::INFINITY),
        );
        edit.request_focus();
        ui.add_space(8.0);
        egui::ScrollArea::vertical().max_height(440.0).show(ui, |ui| {
            for (label, key) in &rows {
                ui.horizontal(|ui| {
                    ui.set_min_height(28.0);
                    ui.label(RichText::new(label).color(p.text));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| key_chip(ui, key));
                });
            }
            if rows.is_empty() {
                ui.label(RichText::new("Nada coincide").color(p.muted));
            }
        });
        ui.add_space(6.0);
        ui.label(
            RichText::new("Los atajos se cambian en Ajustes → Atajos.")
                .color(p.muted)
                .size(11.5),
        );
    });
    if modal.should_close() {
        open = false;
    }
    open
}
