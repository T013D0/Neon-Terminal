//! Pantalla de Ajustes (Ctrl+,): apariencia, terminal, avisos, atajos y avanzado.
//! Cada cambio se aplica al instante y se guarda en settings.toml.

use crate::commands::{bindables, binding_for};
use crate::profiles::Profile;
use crate::settings::{Backdrop, Settings, format_shortcut};
use crate::theme::pal;
use crate::themes::{Theme, parse_hex};
use egui::{Align, CornerRadius, FontId, Layout, RichText, Sense, Stroke, StrokeKind, vec2};

#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum Section {
    #[default]
    Apariencia,
    Terminal,
    Avisos,
    Atajos,
    Actualizaciones,
    Avanzado,
}

#[derive(Default)]
pub struct State {
    pub section: Section,
    /// Acción cuyo atajo se está grabando.
    pub capturing: Option<&'static str>,
    pub import_text: String,
    pub import_msg: Option<(String, bool)>,
}

pub struct Outcome {
    pub changed: bool,
    pub close: bool,
    pub update: Option<UpdateAction>,
}

#[derive(Clone, Copy, PartialEq)]
pub enum UpdateAction {
    Check,
    Apply,
}

/// Dibuja la ventana de ajustes. Modifica `s` en el lugar.
pub fn show(
    ctx: &egui::Context,
    s: &mut Settings,
    st: &mut State,
    profiles: &[Profile],
    upd: &crate::update::UpdateState,
) -> Outcome {
    let before = s.clone();
    let mut close = false;
    let mut update_action = None;

    // Grabando un atajo: la próxima combinación de teclas se asigna.
    if let Some(id) = st.capturing {
        let pressed = ctx.input_mut(|i| {
            let mut found = None;
            i.events.retain(|e| match e {
                egui::Event::Key {
                    key,
                    pressed: true,
                    modifiers,
                    ..
                } if found.is_none() => {
                    found = Some((*key, *modifiers));
                    false
                }
                egui::Event::Text(_) => false,
                _ => true,
            });
            found
        });
        if let Some((key, mods)) = pressed {
            match key {
                egui::Key::Escape if mods.is_none() => {}
                egui::Key::Backspace | egui::Key::Delete if mods.is_none() => {
                    s.keys.insert(id.to_string(), String::new());
                }
                _ => {
                    let sc = egui::KeyboardShortcut::new(mods, key);
                    s.keys.insert(id.to_string(), format_shortcut(&sc));
                }
            }
            st.capturing = None;
        }
    }

    let p = pal();
    let modal = egui::Modal::new(egui::Id::new("neon_settings"))
        .frame(
            egui::Frame::new()
                .fill(p.surface)
                .stroke(Stroke::new(1.0_f32, p.border))
                .corner_radius(CornerRadius::same(14))
                .inner_margin(egui::Margin::same(0)),
        )
        .show(ctx, |ui| {
            // Áreas fijas: menú a la izquierda, contenido con scroll y pie con "Listo".
            let (full, _) = ui.allocate_exact_size(vec2(760.0, 540.0), Sense::hover());
            let nav_rect = egui::Rect::from_min_size(full.min, vec2(190.0, full.height()));
            let content_rect = egui::Rect::from_min_max(
                egui::pos2(nav_rect.max.x + 24.0, full.min.y + 18.0),
                egui::pos2(full.max.x - 20.0, full.max.y - 56.0),
            );
            let footer_rect = egui::Rect::from_min_max(
                egui::pos2(nav_rect.max.x + 24.0, full.max.y - 50.0),
                egui::pos2(full.max.x - 20.0, full.max.y - 10.0),
            );

            ui.painter().rect_filled(
                nav_rect,
                CornerRadius {
                    nw: 14,
                    sw: 14,
                    ne: 0,
                    se: 0,
                },
                p.panel,
            );
            let mut nav = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(nav_rect.shrink2(vec2(12.0, 18.0)))
                    .layout(Layout::top_down(Align::Min)),
            );
            nav.spacing_mut().item_spacing.y = 4.0;
            nav.label(RichText::new("Ajustes").size(18.0).strong());
            nav.add_space(14.0);
            for (sec, name) in [
                (Section::Apariencia, "Apariencia"),
                (Section::Terminal, "Terminal"),
                (Section::Avisos, "Avisos"),
                (Section::Atajos, "Atajos"),
                (Section::Actualizaciones, "Actualizaciones"),
                (Section::Avanzado, "Avanzado"),
            ] {
                let sel = st.section == sec;
                let (r, resp) = nav.allocate_exact_size(vec2(166.0, 32.0), Sense::click());
                if sel {
                    nav.painter().rect_filled(
                        r,
                        CornerRadius::same(8),
                        crate::theme::mix(p.panel, p.red, 0.16),
                    );
                } else if resp.hovered() {
                    nav.painter().rect_filled(r, CornerRadius::same(8), p.hover);
                }
                nav.painter().text(
                    r.left_center() + vec2(12.0, 0.0),
                    egui::Align2::LEFT_CENTER,
                    name,
                    FontId::proportional(14.0),
                    if sel { p.text } else { p.muted },
                );
                if resp.clicked() {
                    st.section = sec;
                    st.capturing = None;
                }
            }

            let mut content = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(content_rect)
                    .layout(Layout::top_down(Align::Min)),
            );
            content.spacing_mut().item_spacing = vec2(8.0, 8.0);
            egui::ScrollArea::vertical()
                .id_salt(("settings", st.section as u8))
                .max_height(content_rect.height())
                .auto_shrink([false, false])
                .show(&mut content, |ui| {
                    ui.set_width(content_rect.width() - 14.0);
                    match st.section {
                        Section::Apariencia => appearance(ui, s, st),
                        Section::Terminal => terminal(ui, s, profiles),
                        Section::Avisos => notices(ui, s),
                        Section::Atajos => shortcuts(ui, s, st),
                        Section::Actualizaciones => {
                            update_action = updates(ui, s, upd);
                        }
                        Section::Avanzado => advanced(ui, s),
                    }
                });

            ui.painter().hline(
                footer_rect.x_range(),
                footer_rect.min.y - 2.0,
                Stroke::new(1.0_f32, p.border),
            );
            let mut footer = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(footer_rect)
                    .layout(Layout::right_to_left(Align::Center)),
            );
            let done = egui::Button::new(RichText::new("Listo").color(egui::Color32::WHITE))
                .fill(p.red)
                .min_size(vec2(84.0, 30.0));
            if footer.add(done).clicked() {
                close = true;
            }
            footer.label(
                RichText::new("Los cambios se guardan solos")
                    .size(11.5)
                    .color(p.muted),
            );
        });
    if modal.should_close() && st.capturing.is_none() {
        close = true;
    }
    Outcome {
        changed: *s != before,
        close,
        update: update_action,
    }
}

fn heading(ui: &mut egui::Ui, text: &str) {
    ui.add_space(6.0);
    ui.label(RichText::new(text).size(15.0).strong());
}

fn hint(ui: &mut egui::Ui, text: &str) {
    ui.label(RichText::new(text).size(12.0).color(pal().muted));
}

fn appearance(ui: &mut egui::Ui, s: &mut Settings, st: &mut State) {
    heading(ui, "Tema");
    let themes = crate::settings::all_themes(s);
    let custom: Vec<String> = s.custom_themes.iter().map(|t| t.name.clone()).collect();
    let mut remove = None;
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(10.0, 10.0);
        for t in &themes {
            let selected = t.name == s.theme;
            let resp = theme_card(ui, t, selected);
            if resp.clicked() {
                s.theme = t.name.clone();
            }
            if custom.contains(&t.name) {
                resp.context_menu(|ui| {
                    if ui.button("Quitar tema").clicked() {
                        remove = Some(t.name.clone());
                        ui.close_menu();
                    }
                });
            }
        }
    });
    if let Some(name) = remove {
        s.custom_themes.retain(|t| t.name != name);
        if s.theme == name {
            s.theme = "Brasa".into();
        }
    }

    ui.collapsing("Importar tema de Windows Terminal", |ui| {
        hint(
            ui,
            "Pega un esquema de \"schemes\" del settings.json de Windows Terminal (o el archivo entero). \
             También sirven los de windowsterminalthemes.dev.",
        );
        ui.add(
            egui::TextEdit::multiline(&mut st.import_text)
                .desired_rows(5)
                .desired_width(f32::INFINITY)
                .font(FontId::monospace(12.0))
                .hint_text("{ \"name\": \"…\", \"background\": \"#…\", … }"),
        );
        ui.horizontal(|ui| {
            if ui.button("Importar").clicked() {
                match crate::themes::import_windows_terminal(&st.import_text) {
                    Ok(list) => {
                        let names: Vec<String> = list.iter().map(|t| t.name.clone()).collect();
                        for t in list {
                            s.custom_themes.retain(|c| c.name != t.name);
                            s.custom_themes.push(t);
                        }
                        s.theme = names[0].clone();
                        st.import_text.clear();
                        st.import_msg = Some((format!("Importado: {}", names.join(", ")), true));
                    }
                    Err(e) => st.import_msg = Some((e, false)),
                }
            }
            if let Some((msg, ok)) = &st.import_msg {
                ui.label(RichText::new(msg).size(12.0).color(if *ok { pal().green } else { pal().red }));
            }
        });
    });

    heading(ui, "Letra de la terminal");
    ui.add(
        egui::Slider::new(&mut s.font_size, 9.0..=28.0)
            .step_by(1.0)
            .suffix(" pt"),
    );

    heading(ui, "Fondo");
    ui.horizontal(|ui| {
        ui.radio_value(&mut s.backdrop, Backdrop::Ninguno, "Sólido");
        ui.radio_value(&mut s.backdrop, Backdrop::Mica, "Mica");
        ui.radio_value(&mut s.backdrop, Backdrop::Acrylic, "Acrylic");
    });
    ui.horizontal(|ui| {
        ui.label("Opacidad");
        ui.add(
            egui::Slider::new(&mut s.opacity, 0.35..=1.0)
                .step_by(0.05)
                .custom_formatter(|v, _| format!("{:.0} %", v * 100.0)),
        );
    });
    if s.backdrop != Backdrop::Ninguno && s.opacity > 0.95 {
        hint(ui, "Baja la opacidad para que se vea el efecto.");
    }
    hint(
        ui,
        "Mica y Acrylic son de Windows 11. Con opacidad menor a 100 % se ve el escritorio o el efecto detrás.",
    );
}

/// Tarjeta con una miniatura del tema.
fn theme_card(ui: &mut egui::Ui, t: &Theme, selected: bool) -> egui::Response {
    let p = t.palette();
    let (r, resp) = ui.allocate_exact_size(vec2(160.0, 92.0), Sense::click());
    let painter = ui.painter();
    painter.rect_filled(r, CornerRadius::same(10), p.bg);
    let inner = r.shrink(8.0);
    let term = egui::Rect::from_min_size(inner.min, vec2(inner.width(), 48.0));
    painter.rect_filled(term, CornerRadius::same(6), p.panel);
    // Una "línea de prompt" de ejemplo.
    painter.rect_filled(
        egui::Rect::from_min_size(term.min + vec2(8.0, 9.0), vec2(42.0, 10.0)),
        CornerRadius::same(5),
        crate::theme::mix(p.panel, p.red, 0.22),
    );
    painter.rect_filled(
        egui::Rect::from_min_size(term.min + vec2(54.0, 9.0), vec2(28.0, 10.0)),
        CornerRadius::same(5),
        crate::theme::mix(p.panel, p.violet, 0.22),
    );
    for (i, c) in t.ansi.iter().skip(1).take(6).enumerate() {
        if let Some(c) = parse_hex(c) {
            painter.circle_filled(term.min + vec2(12.0 + i as f32 * 14.0, 34.0), 4.0, c);
        }
    }
    painter.text(
        egui::pos2(inner.min.x + 2.0, inner.max.y - 8.0),
        egui::Align2::LEFT_CENTER,
        &t.name,
        FontId::proportional(13.0),
        p.text,
    );
    let border = if selected {
        Stroke::new(2.0_f32, pal().red)
    } else if resp.hovered() {
        Stroke::new(1.0_f32, pal().muted)
    } else {
        Stroke::new(1.0_f32, pal().border)
    };
    painter.rect_stroke(r, CornerRadius::same(10), border, StrokeKind::Inside);
    resp
}

fn terminal(ui: &mut egui::Ui, s: &mut Settings, profiles: &[Profile]) {
    heading(ui, "Shell predeterminada");
    let current = s
        .default_profile
        .and_then(|k| profiles.iter().find(|p| p.kind == k))
        .map(|p| p.name)
        .unwrap_or("Automática (la primera disponible)");
    egui::ComboBox::from_id_salt("default_profile")
        .selected_text(current)
        .width(260.0)
        .show_ui(ui, |ui| {
            ui.selectable_value(
                &mut s.default_profile,
                None,
                "Automática (la primera disponible)",
            );
            for p in profiles {
                ui.selectable_value(&mut s.default_profile, Some(p.kind), p.name);
            }
        });

    heading(ui, "Historial de scroll");
    ui.horizontal(|ui| {
        ui.add(
            egui::DragValue::new(&mut s.scrollback)
                .range(1_000..=200_000)
                .speed(500.0),
        );
        ui.label("líneas por panel");
    });
    hint(ui, "Se aplica a los paneles que abras desde ahora.");

    heading(ui, "Animaciones");
    ui.checkbox(
        &mut s.animations,
        "Animaciones suaves (cursor, pestañas, paneles y paleta)",
    );
    ui.checkbox(&mut s.startup_anim, "Animación del logo al abrir Neon");

    heading(ui, "Al abrir y cerrar");
    ui.checkbox(
        &mut s.restore_session,
        "Restaurar ventanas y pestañas al abrir Neon",
    );
    ui.checkbox(
        &mut s.confirm_close,
        "Preguntar antes de cerrar si hay un comando corriendo",
    );

    heading(ui, "Selección y portapapeles");
    ui.checkbox(
        &mut s.copy_on_select,
        "Copiar automáticamente al seleccionar texto",
    );
    ui.checkbox(
        &mut s.mouse_select,
        "Arrastrar selecciona aunque la app use el mouse (Claude Code); con Shift el mouse va a la app",
    );
    ui.checkbox(&mut s.middle_paste, "Clic central pega el portapapeles");
    ui.checkbox(
        &mut s.smart_paste,
        "Ctrl+V inteligente (texto, archivos copiados e imágenes)",
    );

    heading(ui, "Claude Code");
    ui.checkbox(
        &mut s.shift_enter_newline,
        "Shift+Enter inserta un salto de línea cuando corre Claude Code",
    );
    ui.checkbox(
        &mut s.notify_bell,
        "Avisar cuando un panel que no miras llama la atención (campana)",
    );

    heading(ui, "Prompt");
    ui.checkbox(
        &mut s.neon_prompt,
        "Usar el prompt de Neon (cápsulas con carpeta, rama y cambios)",
    );
    hint(
        ui,
        "Si usas oh-my-posh o starship, Neon respeta tu prompt igual. Se aplica a paneles nuevos.",
    );
}

fn notices(ui: &mut egui::Ui, s: &mut Settings) {
    heading(ui, "Comandos largos");
    ui.checkbox(
        &mut s.notify,
        "Avisar cuando termina un comando largo que no estás mirando",
    );
    ui.add_enabled_ui(s.notify, |ui| {
        ui.horizontal(|ui| {
            ui.label("Si tardó más de");
            ui.add(egui::DragValue::new(&mut s.notify_after_secs).range(3..=3600));
            ui.label("segundos");
        });
    });
    hint(
        ui,
        "Aparece una notificación de Windows y el ícono parpadea en la barra de tareas.",
    );
}

fn shortcuts(ui: &mut egui::Ui, s: &mut Settings, st: &mut State) {
    heading(ui, "Atajos de teclado");
    hint(
        ui,
        "Haz click en un atajo y presiona la combinación nueva. Esc cancela, Supr lo deja sin atajo.",
    );
    ui.add_space(4.0);
    let all = bindables();
    // Para avisar si dos acciones comparten atajo.
    let effective: Vec<(String, Option<egui::KeyboardShortcut>)> = all
        .iter()
        .map(|b| (b.label.to_string(), binding_for(b, s)))
        .collect();
    let p = pal();
    egui::Grid::new("keys")
        .num_columns(3)
        .spacing(vec2(12.0, 6.0))
        .show(ui, |ui| {
            for (i, b) in all.iter().enumerate() {
                ui.label(b.label);
                let capturing = st.capturing == Some(b.id);
                let text = if capturing {
                    "Presiona la combinación…".to_string()
                } else {
                    effective[i]
                        .1
                        .map(|sc| format_shortcut(&sc))
                        .unwrap_or_else(|| "—".into())
                };
                let btn = egui::Button::new(
                    RichText::new(text)
                        .monospace()
                        .size(12.5)
                        .color(if capturing { p.red } else { p.text }),
                )
                .min_size(vec2(190.0, 24.0));
                if ui.add(btn).clicked() {
                    st.capturing = Some(b.id);
                }
                ui.horizontal(|ui| {
                    if s.keys.contains_key(b.id) && ui.small_button("Restaurar").clicked() {
                        s.keys.remove(b.id);
                    }
                    if let Some(sc) = effective[i].1 {
                        if let Some((other, _)) = effective
                            .iter()
                            .enumerate()
                            .find(|(j, (_, o))| *j != i && *o == Some(sc))
                            .map(|(_, x)| x)
                        {
                            ui.label(
                                RichText::new(format!("También: {other}"))
                                    .size(11.5)
                                    .color(p.orange),
                            );
                        }
                    }
                });
                ui.end_row();
            }
        });
    ui.add_space(6.0);
    if ui.button("Restaurar todos los atajos").clicked() {
        s.keys.clear();
    }
    hint(
        ui,
        "Fijos: Ctrl+1…9 (ir a la pestaña), Ctrl+Shift+C / Ctrl+Shift+V (copiar / pegar), ↑ en el prompt (historial).",
    );
}

fn updates(
    ui: &mut egui::Ui,
    s: &mut Settings,
    upd: &crate::update::UpdateState,
) -> Option<UpdateAction> {
    use crate::update::UpdateState as U;
    let p = pal();
    let mut action = None;
    heading(ui, "Versión");
    ui.label(format!("Neon v{}", crate::update::VERSION));

    heading(ui, "Actualizaciones desde GitHub");
    ui.checkbox(
        &mut s.check_updates,
        "Buscar actualizaciones al abrir (una vez al día)",
    );
    ui.horizontal(|ui| {
        ui.label("Repositorio");
        ui.label(
            RichText::new(crate::update::DEFAULT_REPO)
                .monospace()
                .color(p.muted),
        );
    });

    ui.add_space(6.0);
    let busy = matches!(upd, U::Checking | U::Downloading(_));
    ui.horizontal(|ui| {
        if ui
            .add_enabled(!busy, egui::Button::new("Buscar ahora"))
            .clicked()
        {
            action = Some(UpdateAction::Check);
        }
        match upd {
            U::Checking => {
                ui.spinner();
                ui.label(RichText::new("Buscando…").color(p.muted));
            }
            U::Downloading(v) => {
                ui.spinner();
                ui.label(RichText::new(format!("Descargando v{v}…")).color(p.muted));
            }
            U::UpToDate => {
                ui.label(RichText::new("Estás al día").color(p.green));
            }
            _ => {}
        }
    });
    if let U::Error(e) = upd {
        ui.label(RichText::new(e).color(egui::Color32::from_rgb(0xff, 0x6b, 0x6b)));
    }
    if let U::Available(r) = upd {
        ui.add_space(8.0);
        egui::Frame::new()
            .fill(p.panel)
            .stroke(Stroke::new(1.0_f32, p.border))
            .corner_radius(CornerRadius::same(10))
            .inner_margin(egui::Margin::same(12))
            .show(ui, |ui| {
                ui.label(
                    RichText::new(format!("Hay una versión nueva: v{}", r.version))
                        .strong()
                        .size(14.0),
                );
                if !r.notes.is_empty() {
                    let notes: String = r.notes.chars().take(500).collect();
                    ui.label(RichText::new(notes).size(12.0).color(p.muted));
                }
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    let go = egui::Button::new(
                        RichText::new("Actualizar ahora").color(egui::Color32::WHITE),
                    )
                    .fill(p.red);
                    if ui.add(go).clicked() {
                        action = Some(UpdateAction::Apply);
                    }
                    if !r.page.is_empty() && ui.button("Ver novedades").clicked() {
                        let _ = open::that_detached(&r.page);
                    }
                });
                hint(
                    ui,
                    "Neon se cierra, se actualiza y vuelve a abrir. Tu sesión se restaura.",
                );
            });
    }
    action
}

fn advanced(ui: &mut egui::Ui, s: &mut Settings) {
    heading(ui, "Archivo de ajustes");
    hint(
        ui,
        "Todo lo de esta pantalla vive en settings.toml. Si lo editas a mano, Neon lo recarga al guardar.",
    );
    ui.label(
        RichText::new(crate::settings::path().display().to_string())
            .monospace()
            .size(12.0),
    );
    ui.horizontal(|ui| {
        if ui.button("Abrir settings.toml").clicked() {
            if !crate::settings::path().exists() {
                crate::settings::save(s);
            }
            let _ = open::that_detached(crate::settings::path());
        }
        if ui.button("Abrir carpeta de datos").clicked() {
            let _ = open::that_detached(crate::shell::data_dir());
        }
    });
    heading(ui, "Empezar de cero");
    if ui.button("Restaurar todos los ajustes").clicked() {
        let themes = std::mem::take(&mut s.custom_themes);
        *s = Settings::default();
        s.custom_themes = themes;
    }
    hint(ui, "Los temas importados se conservan.");
}
