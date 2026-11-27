//! La app: ventanas, pestañas, paneles, arrastrar y soltar, historial,
//! paleta de comandos, sesión, avisos y links.
//!
//! Todas las ventanas viven en el mismo proceso (viewports de egui), así una
//! pestaña se puede arrastrar de una ventana a otra o sacar a una nueva sin
//! cortar la shell que tiene adentro.

use crate::commands::{Cmd, PaletteItem, palette_items};
use crate::history::{Entry, History, NewCommand};
use crate::pane::Pane;
use crate::profiles::{Profile, ProfileKind};
use crate::session::{self, LayoutNode, Session, TabState, WinState};
use crate::settings::{self, Backdrop, Settings};
use crate::theme::{self, pal};
use crate::ui::{self, PaneBehavior, WINDOW_BUTTON_W, WindowButton, now, popup_frame};
use crate::update::{self, UpdateMsg, UpdateState};
use crate::workspace::{self, Dir, DropZone, Node, Workspace, first_pane_id, tab_colors};
use egui::{
    Align, Align2, Color32, CornerRadius, FontId, Frame, Key, Layout, Margin, Modifiers, Pos2,
    Rect, RichText, Sense, Stroke, StrokeKind, ViewportBuilder, ViewportId, pos2, vec2,
};
use egui_term::{FontSettings, PtyEvent, TerminalFont, TerminalTheme};
use egui_tiles::LinearDir;
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::{Duration, Instant};

const BAR_H: f32 = 42.0;
const DEFAULT_FONT: f32 = theme::TERM_FONT_SIZE;

#[derive(PartialEq, Clone, Copy)]
enum HistoryMode {
    /// Lista pegada al cursor, abierta con ↑ en el prompt.
    Inline,
    /// Buscador centrado, abierto con Ctrl+R.
    Search,
}

struct HistoryPopup {
    mode: HistoryMode,
    pane_id: u64,
    query: String,
    results: Vec<Entry>,
    selected: usize,
    computed_for: Option<String>,
}

/// Un panel mandado a segundo plano: su shell sigue corriendo, pero no se
/// dibuja (no gasta nada en pantalla); queda como un chip en la barra.
struct BgProc {
    pane: Pane,
    label: String,
    state: BgState,
}

#[derive(Clone, Copy, PartialEq)]
enum BgState {
    Running,
    /// El comando terminó (bien o mal) y la shell quedó esperando.
    Done(bool),
    /// La shell se cerró.
    Exited,
}

struct CmdPalette {
    query: String,
    selected: usize,
}

/// Una ventana de Neon.
pub struct Win {
    id: u64,
    viewport: ViewportId,
    /// None = ventana principal (la crea eframe).
    builder: Option<ViewportBuilder>,
    workspaces: Vec<Workspace>,
    active: usize,
    popup: Option<HistoryPopup>,
    /// Lista de carpetas abierta al escribir `cd `.
    dirs: Option<crate::dirs::DirPopup>,
    /// Se cerró con Esc: no reabrir hasta que cambie la línea.
    dirs_dismissed: bool,
    /// Se escribió «cd » este frame: la lista se abre en el siguiente (así el
    /// espacio alcanza a llegar a la shell).
    dirs_armed: bool,
    palette: Option<CmdPalette>,
    renaming: Option<(usize, String)>,
    toast: Option<(String, Instant)>,
    // Geometría del último frame (coordenadas globales de pantalla, en puntos).
    inner: Option<Rect>,
    outer: Option<Rect>,
    focused: bool,
    maximized: bool,
    tabbar: Rect,
    tab_rects: Vec<Rect>,
    /// Pestaña recién cerrada: (posición, ancho, desde cuándo) para animar el hueco.
    closing_tab: Option<(usize, f32, Instant)>,
    pane_rects: Vec<(u64, Rect)>,
    /// Pregunta "¿cerrar igual?" pendiente.
    confirm: Option<Confirm>,
    /// Nombre del espacio de trabajo que se está por guardar.
    naming_layout: Option<String>,
    /// Últimas pestañas cerradas (solo su distribución) para reabrirlas.
    closed: Vec<TabState>,
    /// Pegado esperando confirmación: (panel, texto, motivos).
    paste_confirm: Option<(u64, String, Vec<String>)>,
}

/// Confirmación antes de cerrar algo con un comando corriendo.
struct Confirm {
    title: String,
    detail: String,
    yes: Cmd,
}

/// Una pestaña que se está arrastrando.
struct TabDrag {
    win: u64,
    tab: usize,
    title: String,
    color: Color32,
    start: Pos2,
    global: Pos2,
    active: bool,
    released: bool,
}

pub struct App {
    windows: Vec<Win>,
    pending: Vec<(u64, Cmd)>,
    next_pane: u64,
    next_win: u64,
    pty_tx: Sender<(u64, PtyEvent)>,
    pty_rx: Receiver<(u64, PtyEvent)>,
    history: Option<History>,
    history_version: u64,
    term_theme: TerminalTheme,
    font_size: f32,
    term_font: TerminalFont,
    profiles: Vec<Profile>,
    drag: Option<TabDrag>,
    logo: egui::TextureHandle,
    intro: Option<crate::intro::Intro>,
    /// Paneles que se están cerrando (animación) y desde cuándo.
    closing: Vec<(u64, Instant)>,
    /// Atajo global registrado (modo Quake).
    hotkey: Option<crate::hotkey::GlobalHotkey>,
    /// Pedidos a Claude en curso.
    ai_jobs: Vec<crate::ai::AiJob>,
    /// Lo último copiado en Neon (máx. CLIPS_MAX, cada uno recortado a CLIP_BYTES).
    clips: std::collections::VecDeque<String>,
    /// Historial del portapapeles abierto: (ventana, elegido).
    clip_popup: Option<(usize, usize)>,
    /// Paneles en segundo plano.
    background: Vec<BgProc>,
    /// Cuándo devolver memoria al sistema (un rato después de cerrar paneles,
    /// cuando su terminal ya terminó de soltarse).
    trim_at: Option<Instant>,
    /// Buscador de archivos abierto (ventana, buscador).
    finder: Option<(usize, crate::finder::Finder)>,
    /// `.neon.toml` leídos hace poco.
    projects: crate::project::Cache,
    /// Reporte de la vez anterior que Neon se cayó (se muestra una vez).
    crash_report: Option<String>,
    /// Ayuda abierta (recorrido o atajos).
    help: Option<crate::help::Help>,
    /// Lista de puertos abierta (ventana donde se muestra).
    ports: Option<(usize, Vec<crate::ports::PortInfo>, Instant)>,
    error: Option<String>,
    /// Solo la instancia "normal" guarda la sesión (no las abiertas desde el Explorador).
    persist: bool,
    last_save: Instant,
    style_frames: u8,
    settings: Settings,
    settings_mtime: Option<std::time::SystemTime>,
    settings_dirty: Option<Instant>,
    last_settings_check: Instant,
    /// Ventana donde está abierta la pantalla de Ajustes.
    settings_open: Option<u64>,
    settings_state: crate::settings_ui::State,
    /// Atajos ya convertidos (se recalculan al cambiar los ajustes).
    keymap: Vec<(egui::KeyboardShortcut, Cmd)>,
    /// Se confirmó cerrar la app aunque hubiera comandos corriendo.
    allow_close: bool,
    /// Actualizaciones desde GitHub.
    update: UpdateState,
    upd_tx: Sender<UpdateMsg>,
    upd_rx: Receiver<UpdateMsg>,
}

fn term_font(size: f32) -> TerminalFont {
    TerminalFont::new(FontSettings {
        font_type: FontId::monospace(size),
    })
}

fn window_builder(pos: Option<Pos2>, size: egui::Vec2) -> ViewportBuilder {
    let mut b = ViewportBuilder::default()
        .with_title("Neon")
        .with_inner_size(size)
        .with_min_inner_size([520.0, 320.0])
        .with_decorations(false)
        .with_resizable(true)
        .with_transparent(true)
        .with_icon(std::sync::Arc::new(
            eframe::icon_data::from_png_bytes(include_bytes!("../assets/icon-256.png"))
                .unwrap_or_default(),
        ));
    if let Some(p) = pos {
        b = b.with_position(p);
    }
    b
}

impl Win {
    fn new(id: u64, viewport: ViewportId, builder: Option<ViewportBuilder>) -> Self {
        Self {
            id,
            viewport,
            builder,
            workspaces: vec![],
            active: 0,
            popup: None,
            dirs: None,
            dirs_dismissed: false,
            dirs_armed: false,
            palette: None,
            renaming: None,
            toast: None,
            inner: None,
            outer: None,
            focused: false,
            maximized: false,
            tabbar: Rect::NOTHING,
            tab_rects: vec![],
            closing_tab: None,
            pane_rects: vec![],
            confirm: None,
            naming_layout: None,
            closed: vec![],
            paste_confirm: None,
        }
    }

    fn ws(&self) -> Option<&Workspace> {
        self.workspaces.get(self.active)
    }

    fn ws_mut(&mut self) -> Option<&mut Workspace> {
        self.workspaces.get_mut(self.active)
    }

    fn toast(&mut self, msg: impl Into<String>) {
        self.toast = Some((msg.into(), Instant::now()));
    }

    fn insert_tab(&mut self, at: usize, ws: Workspace) {
        let at = at.min(self.workspaces.len());
        self.workspaces.insert(at, ws);
        self.active = at;
    }

    /// Anota la distribución de la pestaña `i` antes de cerrarla.
    fn remember_closed(&mut self, i: usize) {
        let Some(ws) = self.workspaces.get(i) else { return };
        let Some(layout) = ws.tree.root().and_then(|r| session::layout_of(&ws.tree.tiles, r)) else {
            return;
        };
        let focused = ws.pane_ids().iter().position(|id| *id == ws.focused).unwrap_or(0);
        self.closed.push(TabState {
            name: ws.name.clone(),
            color: ws.color,
            focused,
            layout,
        });
        if self.closed.len() > 10 {
            self.closed.remove(0);
        }
    }

    fn remove_tab(&mut self, i: usize) -> Option<Workspace> {
        if i >= self.workspaces.len() {
            return None;
        }
        let ws = self.workspaces.remove(i);
        // El hueco se cierra de a poco en vez de que las otras salten.
        if crate::settings::get().animations && !self.workspaces.is_empty() {
            if let Some(r) = self.tab_rects.get(i) {
                self.closing_tab = Some((i, r.width(), Instant::now()));
            }
        }
        if self.active > i || self.active >= self.workspaces.len() {
            self.active = self.active.saturating_sub(1);
        }
        if self.popup.is_some() {
            self.popup = None;
        }
        Some(ws)
    }
}

impl App {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        session: Option<Session>,
        start_dir: Option<String>,
    ) -> Self {
        theme::install_fonts(&cc.egui_ctx);
        theme::apply_style(&cc.egui_ctx);
        // Ctrl+= / Ctrl+- cambian la letra de la terminal, no el zoom de la interfaz.
        cc.egui_ctx.options_mut(|o| o.zoom_with_keyboard = false);
        let (pty_tx, pty_rx) = mpsc::channel();
        let history = History::open(&crate::shell::data_dir().join("history.db")).ok();
        let logo = {
            let icon = eframe::icon_data::from_png_bytes(include_bytes!("../assets/logo-64.png"))
                .unwrap_or_default();
            let img = egui::ColorImage::from_rgba_unmultiplied(
                [icon.width as usize, icon.height as usize],
                &icon.rgba,
            );
            cc.egui_ctx
                .load_texture("logo", img, egui::TextureOptions::LINEAR)
        };
        let settings = settings::get();
        let font_size = settings.font_size.clamp(8.0, 32.0);
        let (upd_tx, upd_rx) = mpsc::channel();
        let mut app = Self {
            windows: vec![Win::new(0, ViewportId::ROOT, None)],
            pending: vec![],
            next_pane: 1,
            next_win: 1,
            pty_tx,
            pty_rx,
            history,
            history_version: 0,
            term_theme: theme::terminal_theme(1.0),
            font_size,
            term_font: term_font(font_size),
            profiles: crate::profiles::available(),
            drag: None,
            logo,
            intro: None,
            closing: vec![],
            hotkey: None,
            ai_jobs: vec![],
            clips: Default::default(),
            clip_popup: None,
            background: vec![],
            trim_at: None,
            finder: None,
            projects: crate::project::Cache::new(),
            crash_report: crate::crash::take_pending(),
            help: None,
            ports: None,
            error: None,
            persist: start_dir.is_none(),
            last_save: Instant::now(),
            style_frames: 3,
            settings_mtime: settings::modified(),
            settings,
            settings_dirty: None,
            last_settings_check: Instant::now(),
            settings_open: None,
            settings_state: Default::default(),
            keymap: vec![],
            allow_close: false,
            update: UpdateState::Idle,
            upd_tx,
            upd_rx,
        };
        app.apply_settings(&cc.egui_ctx);
        let ctx = cc.egui_ctx.clone();
        if let Some(s) = session {
            app.restore(&ctx, s);
        }
        if app.windows[0].workspaces.is_empty() {
            if let Some(p) = app.spawn_pane(&ctx, start_dir, None) {
                app.windows[0].workspaces.push(Workspace::new(p));
            }
        }
        if app.settings.startup_anim
            && app.settings.animations
            && std::env::var_os("NEON_NO_INTRO").is_none()
        {
            app.intro = crate::intro::Intro::new(&ctx);
        }
        // Una vez al día, busca versiones nuevas en segundo plano.
        if app.settings.check_updates && app.update_enabled() && update::auto_check_due() {
            app.start_check(&ctx, false);
        }
        app
    }

    // ───────────────────────── Actualizaciones ─────────────────────────

    /// Hay un repositorio configurado (o una dirección de prueba).
    fn update_enabled(&self) -> bool {
        update::valid_repo(update::DEFAULT_REPO)
    }

    fn start_check(&mut self, ctx: &egui::Context, manual: bool) {
        if matches!(
            self.update,
            UpdateState::Checking | UpdateState::Downloading(_)
        ) {
            return;
        }
        if !self.update_enabled() {
            if manual {
                self.update = UpdateState::Error(
                    "Escribe el repositorio de GitHub como usuario/nombre.".into(),
                );
            }
            return;
        }
        // Si ya hay una versión disponible, el chequeo automático no la borra.
        if manual || !matches!(self.update, UpdateState::Available(_)) {
            self.update = UpdateState::Checking;
        }
        update::check_async(
            update::DEFAULT_REPO.to_string(),
            manual,
            self.upd_tx.clone(),
            ctx.clone(),
        );
    }

    fn pump_updates(&mut self, ctx: &egui::Context) {
        while let Ok(m) = self.upd_rx.try_recv() {
            match m {
                UpdateMsg::Checked { result, manual } => match result {
                    Ok(Some(r)) => {
                        update::mark_checked();
                        if !matches!(&self.update, UpdateState::Available(old) if old.version == r.version)
                        {
                            self.toast_all(format!(
                                "Neon v{} disponible: Ajustes → Actualizaciones",
                                r.version
                            ));
                        }
                        self.update = UpdateState::Available(r);
                    }
                    Ok(None) => {
                        update::mark_checked();
                        self.update = if manual {
                            UpdateState::UpToDate
                        } else {
                            UpdateState::Idle
                        };
                    }
                    Err(e) => {
                        // El chequeo automático falla en silencio (sin internet, etc.).
                        self.update = if manual {
                            UpdateState::Error(e)
                        } else {
                            UpdateState::Idle
                        };
                    }
                },
                UpdateMsg::Downloaded(Ok(path)) => match update::launch_installer(&path) {
                    Ok(()) => {
                        // El instalador reemplaza a Neon y lo vuelve a abrir: cerramos sin preguntar.
                        self.save_session();
                        self.allow_close = true;
                        ctx.send_viewport_cmd_to(ViewportId::ROOT, egui::ViewportCommand::Close);
                    }
                    Err(e) => {
                        self.update =
                            UpdateState::Error(format!("No pude abrir el instalador: {e}"));
                    }
                },
                UpdateMsg::Downloaded(Err(e)) => self.update = UpdateState::Error(e),
            }
        }
    }

    /// Aviso en todas las ventanas (no sabemos cuál está mirando el usuario).
    fn toast_all(&mut self, msg: impl Into<String>) {
        let msg = msg.into();
        for w in &mut self.windows {
            w.toast(msg.clone());
        }
    }

    fn apply_update(&mut self, ctx: &egui::Context) {
        if let UpdateState::Available(r) = self.update.clone() {
            self.update = UpdateState::Downloading(r.version.clone());
            update::download_async(r, self.upd_tx.clone(), ctx.clone());
        } else {
            self.toast_all("No hay ninguna actualización pendiente.");
        }
    }

    // ───────────────────────── Ajustes ─────────────────────────

    /// Aplica los ajustes actuales: tema, letra, fondo, atajos.
    fn apply_settings(&mut self, ctx: &egui::Context) {
        let s = self.settings.clone();
        settings::set(s.clone());
        theme::set_theme(settings::theme_by_name(&s));
        theme::apply_style(ctx);
        self.term_theme = theme::terminal_theme(self.opacity());
        self.font_size = s.font_size.clamp(8.0, 32.0);
        self.term_font = term_font(self.font_size);
        crate::window::set_style(
            pal().dark,
            match s.backdrop {
                Backdrop::Ninguno => 0,
                Backdrop::Mica => 2,
                Backdrop::Acrylic => 3,
            },
        );
        self.style_frames = self.style_frames.max(2);
        // Atajos: primero los de más modificadores (Ctrl+Shift+Tab antes que Ctrl+Tab).
        let mut km: Vec<(egui::KeyboardShortcut, Cmd)> = crate::commands::bindables()
            .iter()
            .filter_map(|b| crate::commands::binding_for(b, &s).map(|sc| (sc, b.cmd.clone())))
            .collect();
        let weight = |m: &Modifiers| m.ctrl as u8 + m.shift as u8 + m.alt as u8;
        km.sort_by_key(|(sc, _)| std::cmp::Reverse(weight(&sc.modifiers)));
        self.keymap = km;
    }

    /// Opacidad efectiva del fondo.
    fn opacity(&self) -> f32 {
        self.settings.opacity.clamp(0.35, 1.0)
    }

    /// Marca los ajustes para guardarlos en un momento (y los aplica ya).
    fn settings_changed(&mut self, ctx: &egui::Context) {
        self.apply_settings(ctx);
        self.settings_dirty = Some(Instant::now());
    }

    fn settings_housekeeping(&mut self, ctx: &egui::Context) {
        if let Some(t) = self.settings_dirty {
            if t.elapsed() > Duration::from_millis(400) {
                self.settings_mtime = settings::save(&self.settings);
                self.settings_dirty = None;
            }
        }
        // ¿Alguien editó settings.toml a mano?
        if self.last_settings_check.elapsed() > Duration::from_secs(1)
            && self.settings_dirty.is_none()
        {
            self.last_settings_check = Instant::now();
            if let Some((s, mtime)) = settings::reload_if_changed(self.settings_mtime) {
                self.settings_mtime = mtime;
                if s != self.settings {
                    self.settings = s;
                    self.apply_settings(ctx);
                    for w in &mut self.windows {
                        w.toast("Ajustes recargados desde settings.toml");
                    }
                }
            }
        }
    }

    // ───────────────────────── Paneles y ventanas ─────────────────────────

    fn profile(&self, kind: Option<ProfileKind>) -> Profile {
        kind.or(self.settings.default_profile)
            .and_then(|k| self.profiles.iter().find(|p| p.kind == k))
            .or_else(|| self.profiles.first())
            .cloned()
            .expect("siempre hay al menos un perfil")
    }

    fn spawn_pane(
        &mut self,
        ctx: &egui::Context,
        cwd: Option<String>,
        kind: Option<ProfileKind>,
    ) -> Option<Pane> {
        let id = self.next_pane;
        self.next_pane += 1;
        let profile = self.profile(kind);
        match Pane::new(id, ctx, self.pty_tx.clone(), cwd, &profile) {
            Ok(p) => Some(p),
            Err(e) => {
                self.error = Some(format!("No se pudo abrir {}: {e}", profile.name));
                None
            }
        }
    }

    fn new_window(&mut self, pos: Option<Pos2>, size: egui::Vec2, workspaces: Vec<Workspace>) {
        let id = self.next_win;
        self.next_win += 1;
        let viewport = ViewportId::from_hash_of(("neon-window", id));
        let mut w = Win::new(id, viewport, Some(window_builder(pos, size)));
        w.workspaces = workspaces;
        w.active = w.workspaces.len().saturating_sub(1);
        self.windows.push(w);
        self.style_frames = 3;
    }

    /// Ajusta la posición de una ventana nueva para que quede dentro del monitor.
    fn on_screen(ctx: &egui::Context, pos: Pos2, size: egui::Vec2) -> Pos2 {
        match ctx.input(|i| i.viewport().monitor_size) {
            Some(m) => pos2(
                pos.x.clamp(0.0, (m.x - size.x).max(0.0)),
                pos.y.clamp(0.0, (m.y - size.y).max(0.0)),
            ),
            None => pos,
        }
    }

    fn win_index(&self, id: u64) -> Option<usize> {
        self.windows.iter().position(|w| w.id == id)
    }

    /// (ventana, pestaña) donde está un panel.
    fn locate(&self, pane: u64) -> Option<(usize, usize)> {
        self.windows.iter().enumerate().find_map(|(wi, w)| {
            w.workspaces
                .iter()
                .position(|ws| ws.pane(pane).is_some())
                .map(|ti| (wi, ti))
        })
    }

    fn pane_mut(&mut self, id: u64) -> Option<&mut Pane> {
        let (wi, ti) = self.locate(id)?;
        self.windows[wi].workspaces[ti].pane_mut(id)
    }

    fn close_pane(&mut self, ctx: &egui::Context, id: u64) {
        let Some((wi, ti)) = self.locate(id) else {
            return;
        };
        // Con más paneles en la pestaña, se desvanece antes de irse.
        if self.settings.animations && !self.closing.iter().any(|(c, _)| *c == id) {
            let ws = &mut self.windows[wi].workspaces[ti];
            if ws.pane_ids().len() > 1 {
                if let Some(p) = ws.pane_mut(id) {
                    p.closing = Some(Instant::now());
                    self.closing.push((id, Instant::now()));
                    ctx.request_repaint();
                    return;
                }
            }
        }
        self.closing.retain(|(c, _)| *c != id);
        self.close_pane_now(ctx, wi, ti, id);
    }

    fn close_pane_now(&mut self, ctx: &egui::Context, wi: usize, ti: usize, id: u64) {
        let w = &mut self.windows[wi];
        if w.popup.as_ref().is_some_and(|p| p.pane_id == id) {
            w.popup = None;
        }
        if w.workspaces[ti].pane_ids().len() == 1 {
            w.remember_closed(ti);
        }
        drop(w.workspaces[ti].take_pane(id));
        self.trim_at = Some(Instant::now() + Duration::from_secs(2));
        let w = &mut self.windows[wi];
        if w.workspaces[ti].is_empty() {
            w.remove_tab(ti);
            self.after_tab_removed(ctx, wi);
        }
    }

    /// Si una ventana quedó sin pestañas: se cierra. Si era la principal y hay
    /// otras, la principal toma el lugar de la siguiente.
    fn after_tab_removed(&mut self, ctx: &egui::Context, wi: usize) {
        if !self.windows[wi].workspaces.is_empty() {
            return;
        }
        if wi > 0 {
            self.windows.remove(wi);
        } else if self.windows.len() > 1 {
            self.promote_second_window(ctx);
        } else {
            ctx.send_viewport_cmd_to(ViewportId::ROOT, egui::ViewportCommand::Close);
        }
    }

    /// La ventana principal no se puede cerrar mientras haya otras: toma las
    /// pestañas y la posición de la segunda, que desaparece.
    fn promote_second_window(&mut self, ctx: &egui::Context) {
        let second = self.windows.remove(1);
        let root = &mut self.windows[0];
        root.workspaces = second.workspaces;
        root.active = second.active;
        root.popup = None;
        root.palette = None;
        root.renaming = None;
        if let Some(o) = second.outer {
            ctx.send_viewport_cmd_to(
                ViewportId::ROOT,
                egui::ViewportCommand::OuterPosition(o.min),
            );
        }
        if let Some(i) = second.inner {
            ctx.send_viewport_cmd_to(ViewportId::ROOT, egui::ViewportCommand::InnerSize(i.size()));
        }
    }

    // ───────────────────────── Sesión ─────────────────────────

    fn restore(&mut self, ctx: &egui::Context, s: Session) {
        for (i, ws_state) in s.windows.into_iter().enumerate() {
            let mut tabs = vec![];
            for t in &ws_state.tabs {
                let mut ids = vec![];
                if let Some(node) = self.build_node(ctx, &t.layout, &mut ids) {
                    let focused = ids
                        .get(t.focused)
                        .copied()
                        .or(first_pane_id(&node))
                        .unwrap_or(0);
                    let mut ws = Workspace::from_node(node, focused);
                    ws.name = t.name.clone();
                    ws.color = t.color;
                    tabs.push(ws);
                }
            }
            if tabs.is_empty() {
                continue;
            }
            let active = ws_state.active.min(tabs.len() - 1);
            if i == 0 {
                self.windows[0].workspaces = tabs;
                self.windows[0].active = active;
            } else {
                let size = ws_state
                    .size
                    .map(|s| vec2(s[0], s[1]))
                    .unwrap_or(vec2(1000.0, 680.0));
                self.new_window(ws_state.pos.map(|p| pos2(p[0], p[1])), size, tabs);
                if let Some(w) = self.windows.last_mut() {
                    w.active = active;
                }
            }
        }
    }

    fn build_node(
        &mut self,
        ctx: &egui::Context,
        l: &LayoutNode,
        ids: &mut Vec<u64>,
    ) -> Option<Node> {
        match l {
            LayoutNode::Pane { cwd, profile, run } => {
                let mut p = self.spawn_pane(ctx, Some(cwd.clone()), Some(*profile))?;
                p.pending_run = run.clone();
                ids.push(p.id);
                Some(Node::Pane(p))
            }
            LayoutNode::Split {
                horizontal,
                children,
            } => {
                let kids: Vec<(f32, Node)> = children
                    .iter()
                    .filter_map(|(s, c)| self.build_node(ctx, c, ids).map(|n| (*s, n)))
                    .collect();
                match kids.len() {
                    0 => None,
                    1 => kids.into_iter().next().map(|(_, n)| n),
                    _ => Some(Node::Split {
                        dir: if *horizontal {
                            LinearDir::Horizontal
                        } else {
                            LinearDir::Vertical
                        },
                        children: kids,
                    }),
                }
            }
        }
    }

    fn snapshot(&self) -> Session {
        let windows = self
            .windows
            .iter()
            .map(|w| WinState {
                pos: w.outer.map(|r| [r.min.x, r.min.y]),
                size: w.inner.map(|r| [r.width(), r.height()]),
                maximized: w.maximized,
                active: w.active,
                tabs: w
                    .workspaces
                    .iter()
                    .filter_map(|ws| {
                        let root = ws.tree.root()?;
                        let layout = session::layout_of(&ws.tree.tiles, root)?;
                        let focused = ws
                            .pane_ids()
                            .iter()
                            .position(|id| *id == ws.focused)
                            .unwrap_or(0);
                        Some(TabState {
                            name: ws.name.clone(),
                            color: ws.color,
                            focused,
                            layout,
                        })
                    })
                    .collect(),
            })
            .filter(|w| !w.tabs.is_empty())
            .collect();
        Session {
            version: 1,
            font_size: self.font_size,
            windows,
        }
    }

    fn save_session(&mut self) {
        if self.persist {
            session::save(&self.snapshot());
        }
        self.last_save = Instant::now();
    }

    // ───────────────────────── Eventos ─────────────────────────

    /// Eventos de las PTY y de las shells; avisos de comandos largos; links.
    fn pump_events(&mut self, ctx: &egui::Context) {
        let mut exited = vec![];
        let mut bells: Vec<u64> = vec![];
        let mut woke: Vec<u64> = vec![];
        while let Ok((id, ev)) = self.pty_rx.try_recv() {
            match ev {
                PtyEvent::Bell => bells.push(id),
                // OSC 52: la app (p. ej. Claude Code) pide copiar al portapapeles.
                PtyEvent::ClipboardStore(_, text) => ctx.copy_text(text),
                PtyEvent::Wakeup => woke.push(id),
                PtyEvent::Exit | PtyEvent::ChildExit(_) => exited.push(id),
                PtyEvent::Title(t) => {
                    if let Some(p) = self.pane_mut(id) {
                        p.title = Some(t);
                    }
                }
                _ => {}
            }
        }
        for id in exited {
            if let Some(b) = self.background.iter_mut().find(|b| b.pane.id == id) {
                if b.state != BgState::Exited {
                    b.state = BgState::Exited;
                    notify("Proceso en segundo plano detenido", format!("{} se cerró", b.label));
                    self.windows[0].toast(format!("{} (segundo plano) se cerró", b.label));
                }
            } else {
                self.close_pane(ctx, id);
            }
        }
        // Paneles en segundo plano: solo se leen sus eventos (comando terminado).
        let mut bg_toasts = vec![];
        for b in &mut self.background {
            for f in b.pane.poll_events() {
                let ok = f.event.ok.unwrap_or(true);
                b.state = BgState::Done(ok);
                if !ok {
                    notify("Falló un proceso en segundo plano", b.label.clone());
                    bg_toasts.push(format!("✕ {} falló (segundo plano)", b.label));
                }
            }
        }
        for t in bg_toasts {
            self.windows[0].toast(t);
        }

        let mut recorded = false;
        let history = self.history.as_ref();
        let notify_on = self.settings.notify;
        let notify_bell = self.settings.notify_bell;
        let long_cmd = Duration::from_secs(self.settings.notify_after_secs.max(1));
        for w in &mut self.windows {
            let mut toast = None;
            for (ti, ws) in w.workspaces.iter_mut().enumerate() {
                let visible = w.focused && ti == w.active;
                let shown = ti == w.active;
                for p in ws.panes_mut() {
                    if woke.contains(&p.id) {
                        p.last_output = Some(Instant::now());
                    }
                    p.track_claude(visible);
                    if shown {
                        p.activity = false;
                        p.cmd_done = None;
                    }
                    if visible {
                        p.attention = false;
                    } else {
                        if woke.contains(&p.id) && !shown {
                            p.activity = true;
                        }
                        if bells.contains(&p.id) && !p.attention {
                            p.attention = true;
                            if notify_bell {
                                notify(
                                    if p.is_claude() {
                                        "Claude Code te necesita"
                                    } else {
                                        "Aviso de la terminal"
                                    },
                                    format!("{} · {}", p.display_title(), ui::short_path(&p.cwd)),
                                );
                                ctx.send_viewport_cmd_to(
                                    w.viewport,
                                    egui::ViewportCommand::RequestUserAttention(
                                        egui::UserAttentionType::Informational,
                                    ),
                                );
                            }
                        }
                    }
                    for f in p.poll_events() {
                        let e = &f.event;
                        if !shown {
                            p.cmd_done = Some(e.ok.unwrap_or(true));
                        }
                        if let (Some(h), Some(cmd)) = (history, e.command.as_deref()) {
                            let cwd = e.cwd.clone().unwrap_or_else(|| p.cwd.clone());
                            let repo = p.git.status.as_ref().map(|g| g.root.clone());
                            let _ = h.record(&NewCommand {
                                command: cmd,
                                cwd: &cwd,
                                repo_root: repo.as_deref(),
                                ok: e.ok.unwrap_or(true),
                                exit_code: e.exit_code,
                                duration_ms: e
                                    .duration_ms
                                    .or(f.elapsed.map(|d| d.as_millis() as i64)),
                                ts: e.ts.unwrap_or_else(now),
                            });
                            recorded = true;
                        }
                        // Pediste aviso para este comando.
                        if std::mem::take(&mut p.notify_done) {
                            let ok = e.ok.unwrap_or(true);
                            notify(
                                if ok { "✓ Terminó" } else { "✕ Falló" },
                                format!(
                                    "{}\n{} · {}",
                                    e.command.as_deref().unwrap_or(""),
                                    ui::fmt_ms(f.elapsed.map(|d| d.as_millis() as i64)),
                                    ui::short_path(&p.cwd)
                                ),
                            );
                            ctx.send_viewport_cmd_to(
                                w.viewport,
                                egui::ViewportCommand::RequestUserAttention(
                                    egui::UserAttentionType::Informational,
                                ),
                            );
                            continue;
                        }
                        // Aviso si tardó y no lo estabas mirando.
                        if notify_on && f.elapsed.is_some_and(|d| d >= long_cmd) && !visible {
                            let ok = e.ok.unwrap_or(true);
                            let secs = f.elapsed.unwrap().as_secs();
                            notify(
                                if ok {
                                    "Comando terminado"
                                } else {
                                    "El comando falló"
                                },
                                format!(
                                    "{}\n{} · {}",
                                    e.command.as_deref().unwrap_or(""),
                                    ui::fmt_ms(Some(secs as i64 * 1000)),
                                    ui::short_path(&p.cwd)
                                ),
                            );
                            ctx.send_viewport_cmd_to(
                                w.viewport,
                                egui::ViewportCommand::RequestUserAttention(
                                    egui::UserAttentionType::Informational,
                                ),
                            );
                        }
                    }
                    let cwd = p.cwd.clone();
                    p.git.tick(&cwd, ctx);
                    // Ctrl+click en un link o una ruta.
                    if let Some(link) = p.backend.take_pending_link() {
                        if let Some(err) = p.open_link(&link) {
                            toast = Some(err);
                        }
                    }
                }
            }
            if let Some(t) = toast {
                w.toast(t);
            }
        }
        if recorded {
            self.history_version += 1;
        }
    }

    // ───────────────────────── Comandos ─────────────────────────

    fn exec(&mut self, ctx: &egui::Context, win_id: u64, cmd: Cmd) {
        let Some(wi) = self.win_index(win_id) else {
            return;
        };
        let focused_cwd = self.windows[wi]
            .ws()
            .and_then(|ws| ws.focused_pane())
            .map(|p| p.cwd.clone());
        let focused_profile = self.windows[wi]
            .ws()
            .and_then(|ws| ws.focused_pane())
            .map(|p| p.profile);
        let is_run = matches!(cmd, Cmd::Run(..));
        match cmd {
            Cmd::NewTab(kind) => {
                if let Some(p) = self.spawn_pane(ctx, focused_cwd, kind) {
                    let w = &mut self.windows[wi];
                    let at = w.workspaces.len();
                    w.insert_tab(at, Workspace::new(p));
                }
            }
            Cmd::ClaudeSplit => {
                if let Some(mut p) = self.spawn_pane(ctx, focused_cwd, focused_profile) {
                    p.pending_run = Some("claude".into());
                    let new_id = p.id;
                    if let Some(ws) = self.windows[wi].ws_mut() {
                        let target = ws.focused;
                        if ws.insert_next_to(target, Node::Pane(p), LinearDir::Horizontal, false) {
                            ws.focused = new_id;
                        }
                    }
                }
            }
            Cmd::SplitRight | Cmd::SplitDown => {
                let dir = if cmd == Cmd::SplitRight {
                    LinearDir::Horizontal
                } else {
                    LinearDir::Vertical
                };
                if let Some(p) = self.spawn_pane(ctx, focused_cwd, focused_profile) {
                    let new_id = p.id;
                    if let Some(ws) = self.windows[wi].ws_mut() {
                        let target = ws.focused;
                        if ws.insert_next_to(target, Node::Pane(p), dir, false) {
                            ws.focused = new_id;
                        }
                    }
                }
            }
            Cmd::ClosePane => {
                if let Some(id) = self.windows[wi].ws().map(|ws| ws.focused) {
                    let running = self.windows[wi]
                        .ws()
                        .and_then(|ws| ws.pane(id))
                        .and_then(|p| p.running());
                    match running.filter(|_| self.settings.confirm_close) {
                        Some(cmd) => {
                            self.windows[wi].confirm = Some(Confirm {
                                title: "¿Cerrar el panel?".into(),
                                detail: format!("Todavía está corriendo:  {cmd}"),
                                yes: Cmd::ForceClosePane(id),
                            })
                        }
                        None => self.close_pane(ctx, id),
                    }
                }
            }
            Cmd::ForceClosePane(id) => self.close_pane(ctx, id),
            Cmd::CloseTab(i) => {
                let running: Vec<String> = self.windows[wi]
                    .workspaces
                    .get(i)
                    .map(|ws| {
                        ws.pane_ids()
                            .iter()
                            .filter_map(|id| ws.pane(*id)?.running())
                            .collect()
                    })
                    .unwrap_or_default();
                if self.settings.confirm_close && !running.is_empty() {
                    self.windows[wi].confirm = Some(Confirm {
                        title: "¿Cerrar la pestaña?".into(),
                        detail: format!("Todavía está corriendo:  {}", running.join(", ")),
                        yes: Cmd::ForceCloseTab(i),
                    });
                } else {
                    self.windows[wi].remember_closed(i);
                    if self.windows[wi].remove_tab(i).is_some() {
                        self.after_tab_removed(ctx, wi);
                    }
                }
            }
            Cmd::ForceCloseTab(i) => {
                self.windows[wi].remember_closed(i);
                if self.windows[wi].remove_tab(i).is_some() {
                    self.after_tab_removed(ctx, wi);
                }
            }
            Cmd::ShowShortcuts => {
                self.help = Some(crate::help::Help::Shortcuts(String::new()));
            }
            Cmd::ShowTour => {
                self.help = Some(crate::help::Help::Tour(0));
            }
            Cmd::ReopenTab => {
                let Some(t) = self.windows[wi].closed.pop() else {
                    self.windows[wi].toast("No hay pestañas cerradas para reabrir");
                    return;
                };
                let mut ids = vec![];
                if let Some(node) = self.build_node(ctx, &t.layout, &mut ids) {
                    let focused = ids.get(t.focused).copied().or(first_pane_id(&node)).unwrap_or(0);
                    let mut ws = Workspace::from_node(node, focused);
                    ws.name = t.name.clone();
                    ws.color = t.color;
                    let w = &mut self.windows[wi];
                    w.workspaces.push(ws);
                    w.active = w.workspaces.len() - 1;
                }
            }
            Cmd::RunHere(c) => {
                let msg = match self.windows[wi].ws_mut().and_then(|ws| ws.focused_pane_mut()) {
                    Some(p) if p.at_prompt || !p.profile.has_integration() => {
                        p.run(&c);
                        None
                    }
                    Some(_) => Some("Hay un comando corriendo en este panel; espera a que termine"),
                    None => None,
                };
                if let Some(m) = msg {
                    self.windows[wi].toast(m);
                }
            }
            Cmd::NewWindow => {
                if let Some(p) = self.spawn_pane(ctx, focused_cwd, None) {
                    let w = &self.windows[wi];
                    let size = w.inner.map(|r| r.size()).unwrap_or(vec2(1000.0, 680.0));
                    let pos = w
                        .outer
                        .map(|r| Self::on_screen(ctx, r.min + vec2(36.0, 36.0), size));
                    self.new_window(pos, size, vec![Workspace::new(p)]);
                }
            }
            Cmd::CloseWindow | Cmd::ForceCloseWindow => {
                let running = self.running_in_window(wi);
                if cmd == Cmd::CloseWindow && self.settings.confirm_close && !running.is_empty() {
                    self.windows[wi].confirm = Some(Confirm {
                        title: "¿Cerrar la ventana?".into(),
                        detail: format!("Todavía está corriendo:  {}", running.join(", ")),
                        yes: Cmd::ForceCloseWindow,
                    });
                    return;
                }
                if wi == 0 {
                    self.allow_close = self.windows.len() == 1;
                    if self.windows.len() > 1 {
                        self.promote_second_window(ctx);
                    } else {
                        ctx.send_viewport_cmd_to(ViewportId::ROOT, egui::ViewportCommand::Close);
                    }
                } else {
                    self.windows.remove(wi);
                }
            }
            Cmd::NextTab | Cmd::PrevTab => {
                let w = &mut self.windows[wi];
                let n = w.workspaces.len();
                if n > 0 {
                    w.active = if cmd == Cmd::NextTab {
                        (w.active + 1) % n
                    } else {
                        (w.active + n - 1) % n
                    };
                }
            }
            Cmd::SelectTab(i) => {
                let w = &mut self.windows[wi];
                if i < w.workspaces.len() {
                    w.active = i;
                }
            }
            Cmd::FocusDir(d) => {
                let w = &mut self.windows[wi];
                let rects = w.pane_rects.clone();
                if let Some(ws) = w.ws_mut() {
                    if ws.zoomed.is_none() {
                        if let Some(n) = workspace::neighbor(&rects, ws.focused, d) {
                            ws.focused = n;
                        }
                    }
                }
            }
            Cmd::ResizeDir(d) => {
                if let Some(ws) = self.windows[wi].ws_mut() {
                    ws.resize_focused(d);
                }
            }
            Cmd::FocusPane(id) => {
                if let Some(ws) = self.windows[wi].ws_mut() {
                    if ws.pane(id).is_some() {
                        ws.focused = id;
                    }
                }
            }
            Cmd::ToggleZoom => {
                if let Some(ws) = self.windows[wi].ws_mut() {
                    ws.zoomed = match ws.zoomed {
                        Some(_) => None,
                        None if ws.pane_ids().len() > 1 => Some(ws.focused),
                        None => None,
                    };
                }
            }
            Cmd::FontBigger | Cmd::FontSmaller | Cmd::FontReset => {
                self.settings.font_size = match cmd {
                    Cmd::FontBigger => (self.font_size + 1.0).min(28.0),
                    Cmd::FontSmaller => (self.font_size - 1.0).max(9.0),
                    _ => DEFAULT_FONT,
                };
                self.settings_changed(ctx);
                let size = self.font_size;
                self.windows[wi].toast(format!("Letra: {size:.0} pt"));
            }
            Cmd::ExplainBlock(pid, idx) => self.explain_block(ctx, pid, idx),
            Cmd::ExplainLast => {
                let target = self.windows[wi].ws().and_then(|ws| ws.focused_pane()).and_then(|p| {
                    p.blocks.iter().rposition(|b| b.ok == Some(false)).map(|i| (p.id, i))
                });
                match target {
                    Some((pid, idx)) => self.explain_block(ctx, pid, idx),
                    None => self.windows[wi].toast("No hay comandos que hayan fallado en este panel"),
                }
            }
            Cmd::ExplainInClaude(pid) => {
                // El contexto va a un archivo y Claude Code lo lee: así no hay
                // problemas de comillas con ninguna shell.
                let Some(note) = self.pane_mut(pid).and_then(|p| p.ai_note.take()) else {
                    return;
                };
                let file = crate::shell::data_dir().join("ultimo-error.md");
                let text = format!(
                    "# Comando que falló\n\n```\n{}\n```\n\n# Salida\n\n```\n{}\n```\n\n# Lo que dijo Claude\n\n{}\n",
                    note.command,
                    note.output,
                    note.body.unwrap_or_default()
                );
                let _ = std::fs::create_dir_all(crate::shell::data_dir());
                if std::fs::write(&file, text).is_err() {
                    self.windows[wi].toast("No pude preparar el contexto para Claude Code");
                    return;
                }
                let (cwd, profile) = self
                    .pane_mut(pid)
                    .map(|p| (Some(p.cwd.clone()), Some(p.profile)))
                    .unwrap_or((focused_cwd, focused_profile));
                if let Some(mut p) = self.spawn_pane(ctx, cwd, profile) {
                    p.pending_run = Some(format!(
                        "claude \"Lee {} y ayúdame a arreglar ese error\"",
                        file.display()
                    ));
                    let new_id = p.id;
                    if let Some(ws) = self.windows[wi].ws_mut() {
                        let target = ws.focused;
                        if ws.insert_next_to(target, Node::Pane(p), LinearDir::Horizontal, false) {
                            ws.focused = new_id;
                        }
                    }
                }
            }
            Cmd::SendToBackground => {
                let w = &mut self.windows[wi];
                let ti = w.active.min(w.workspaces.len().saturating_sub(1));
                let Some(ws) = w.workspaces.get_mut(ti) else { return };
                let id = ws.focused;
                // Si es lo único abierto, primero una pestaña nueva (Neon no se cierra).
                let alone = w.workspaces.len() == 1 && w.workspaces[ti].pane_ids().len() == 1;
                if alone {
                    if let Some(p) = self.spawn_pane(ctx, focused_cwd.clone(), focused_profile) {
                        self.windows[wi].workspaces.push(Workspace::new(p));
                    }
                }
                let w = &mut self.windows[wi];
                let Some(pane) = w.workspaces[ti].take_pane(id) else { return };
                let label = pane
                    .running()
                    .unwrap_or_else(|| pane.display_title());
                if w.workspaces[ti].is_empty() {
                    w.remove_tab(ti);
                    self.after_tab_removed(ctx, wi);
                }
                self.windows[wi].toast(format!("«{label}» sigue corriendo en segundo plano"));
                self.background.push(BgProc { pane, label, state: BgState::Running });
            }
            Cmd::BgRestore(id) => {
                if let Some(i) = self.background.iter().position(|b| b.pane.id == id) {
                    let b = self.background.remove(i);
                    let w = &mut self.windows[wi];
                    w.workspaces.push(Workspace::new(b.pane));
                    w.active = w.workspaces.len() - 1;
                }
            }
            Cmd::BgStop(id) => {
                // Al soltar el panel se cierra su terminal y el proceso termina.
                if let Some(i) = self.background.iter().position(|b| b.pane.id == id) {
                    let b = self.background.remove(i);
                    self.windows[wi].toast(format!("Detenido: {}", b.label));
                    self.trim_at = Some(Instant::now() + Duration::from_secs(2));
                }
            }
            Cmd::FindFile => {
                if self.finder.take().is_none() {
                    let cwd = focused_cwd.clone().unwrap_or_else(|| ".".into());
                    self.finder = Some((wi, crate::finder::Finder::open(&cwd)));
                }
            }
            Cmd::NotifyWhenDone(id) => {
                let id = if id == 0 { self.windows[wi].ws().map_or(0, |ws| ws.focused) } else { id };
                let mut msg = "No hay un comando corriendo en este panel".to_string();
                if let Some(p) = self.pane_mut(id) {
                    if p.running().is_some() {
                        p.notify_done = !p.notify_done;
                        msg = if p.notify_done {
                            "Te aviso cuando termine".into()
                        } else {
                            "Aviso cancelado".into()
                        };
                    }
                }
                self.windows[wi].toast(msg);
            }
            Cmd::Ssh(host) => {
                if let Some(mut p) = self.spawn_pane(ctx, None, None) {
                    p.pending_run = Some(format!("ssh {host}"));
                    let w = &mut self.windows[wi];
                    let mut ws = Workspace::new(p);
                    ws.name = Some(host.clone());
                    w.workspaces.push(ws);
                    w.active = w.workspaces.len() - 1;
                }
            }
            Cmd::ClaudeChanges => {
                let mut msg = None;
                if let Some(p) = self.windows[wi].ws_mut().and_then(|ws| ws.focused_pane_mut()) {
                    if p.changes.take().is_none() {
                        let root = p
                            .git
                            .status
                            .as_ref()
                            .map(|g| std::path::PathBuf::from(&g.root))
                            .or_else(|| crate::changes::repo_root(&p.cwd));
                        match root {
                            Some(r) => {
                                p.changes = Some(crate::changes::Changes::open(r, p.claude_base.as_ref()))
                            }
                            None => msg = Some("Esta carpeta no es un repositorio git"),
                        }
                    }
                }
                if let Some(m) = msg {
                    self.windows[wi].toast(m);
                }
            }
            Cmd::ClipHistory => {
                self.clip_popup = if self.clip_popup.is_some() { None } else { Some((wi, 0)) };
            }
            Cmd::Ports => {
                self.ports = Some((wi, crate::ports::list(), Instant::now()));
            }
            Cmd::StartSaveLayout => {
                let w = &mut self.windows[wi];
                let suggested = w
                    .ws()
                    .and_then(|ws| ws.focused_pane())
                    .and_then(|p| {
                        std::path::Path::new(&p.cwd)
                            .file_name()
                            .map(|n| n.to_string_lossy().into_owned())
                    })
                    .unwrap_or_default();
                w.palette = None;
                w.naming_layout = Some(suggested);
            }
            Cmd::SaveLayout(name) => {
                let w = &self.windows[wi];
                let tabs: Vec<TabState> = w
                    .workspaces
                    .iter()
                    .filter_map(|ws| {
                        let root = ws.tree.root()?;
                        let layout = session::layout_with_commands(&ws.tree.tiles, root)?;
                        let focused = ws
                            .pane_ids()
                            .iter()
                            .position(|id| *id == ws.focused)
                            .unwrap_or(0);
                        Some(TabState {
                            name: ws.name.clone(),
                            color: ws.color,
                            focused,
                            layout,
                        })
                    })
                    .collect();
                let n = tabs.len();
                session::upsert_layout(session::NamedLayout {
                    name: name.clone(),
                    active: w.active,
                    tabs,
                });
                self.windows[wi].toast(format!(
                    "Espacio «{name}» guardado ({n} pestaña{})",
                    if n == 1 { "" } else { "s" }
                ));
            }
            Cmd::OpenLayout(name) => {
                let Some(l) = session::load_layouts().into_iter().find(|l| l.name == name) else {
                    self.windows[wi].toast(format!("No encontré el espacio «{name}»"));
                    return;
                };
                let mut tabs = vec![];
                for t in &l.tabs {
                    let mut ids = vec![];
                    if let Some(node) = self.build_node(ctx, &t.layout, &mut ids) {
                        let focused = ids
                            .get(t.focused)
                            .copied()
                            .or(first_pane_id(&node))
                            .unwrap_or(0);
                        let mut ws = Workspace::from_node(node, focused);
                        ws.name = t.name.clone();
                        ws.color = t.color;
                        tabs.push(ws);
                    }
                }
                let w = &mut self.windows[wi];
                let first = w.workspaces.len();
                let n = tabs.len();
                w.workspaces.extend(tabs);
                if n > 0 {
                    w.active = first + l.active.min(n - 1);
                }
                w.toast(format!("Espacio «{name}» abierto"));
            }
            Cmd::DeleteLayout(name) => {
                session::delete_layout(&name);
                self.windows[wi].toast(format!("Espacio «{name}» borrado"));
            }
            Cmd::QuickSelect => {
                let mut found = true;
                if let Some(p) = self.windows[wi].ws_mut().and_then(|ws| ws.focused_pane_mut()) {
                    if p.quick.take().is_none() {
                        p.quick = crate::quick::QuickSelect::new(&p.screen_rows());
                        found = p.quick.is_some();
                    }
                }
                if !found {
                    self.windows[wi].toast("No hay URLs, rutas ni hashes en pantalla".to_string());
                }
            }
            Cmd::CopyLastOutput => {
                let result = self.windows[wi]
                    .ws()
                    .and_then(|ws| ws.focused_pane())
                    .map(|p| p.last_output());
                let w = &mut self.windows[wi];
                match result {
                    Some(Ok(text)) => {
                        let lines = text.lines().count();
                        ctx.copy_text(text);
                        w.toast(format!(
                            "Salida copiada ({lines} línea{})",
                            if lines == 1 { "" } else { "s" }
                        ));
                    }
                    Some(Err(msg)) => w.toast(msg),
                    None => {}
                }
            }
            Cmd::HistorySearch => self.open_history(wi, HistoryMode::Search),
            Cmd::Palette => {
                let w = &mut self.windows[wi];
                w.popup = None;
                w.palette = Some(CmdPalette {
                    query: String::new(),
                    selected: 0,
                });
            }
            Cmd::StartRename(i) => {
                let w = &mut self.windows[wi];
                if let Some(ws) = w.workspaces.get(i) {
                    // Empieza vacío (o con el nombre puesto); el título actual queda de pista.
                    w.renaming = Some((i, ws.name.clone().unwrap_or_default()));
                }
            }
            Cmd::Rename(i, name) => {
                if let Some(ws) = self.windows[wi].workspaces.get_mut(i) {
                    let n = name.trim();
                    ws.name = (!n.is_empty()).then(|| n.to_string());
                }
            }
            Cmd::SetColor(i, c) => {
                if let Some(ws) = self.windows[wi].workspaces.get_mut(i) {
                    ws.color = c;
                }
            }
            Cmd::JoinTab { from, into, side } => {
                let n = self.windows[wi].workspaces.len();
                if from == into || from >= n || into >= n {
                    return;
                }
                let Some(ws) = self.windows[wi].remove_tab(from) else {
                    return;
                };
                let into = if from < into { into - 1 } else { into };
                let focused = ws.focused;
                if let Some(node) = ws.into_node() {
                    let (dir, before) = match side {
                        Dir::Left => (LinearDir::Horizontal, true),
                        Dir::Right => (LinearDir::Horizontal, false),
                        Dir::Up => (LinearDir::Vertical, true),
                        Dir::Down => (LinearDir::Vertical, false),
                    };
                    let w = &mut self.windows[wi];
                    if let Some(tws) = w.workspaces.get_mut(into) {
                        if tws.insert_at_root(node, dir, before) {
                            tws.focused = focused;
                        }
                    }
                    w.active = into;
                }
            }
            Cmd::TabToNewWindow(i) => {
                if self.windows[wi].workspaces.len() < 2 {
                    self.windows[wi].toast("Es la única pestaña de esta ventana");
                } else if let Some(ws) = self.windows[wi].remove_tab(i) {
                    let w = &self.windows[wi];
                    let size = w.inner.map(|r| r.size()).unwrap_or(vec2(1000.0, 680.0));
                    let pos = w
                        .outer
                        .map(|r| Self::on_screen(ctx, r.min + vec2(48.0, 48.0), size));
                    self.new_window(pos, size, vec![ws]);
                }
            }
            Cmd::PaneToNewTab => {
                let w = &mut self.windows[wi];
                let Some(ws) = w.ws_mut() else { return };
                if ws.pane_ids().len() < 2 {
                    w.toast("La pestaña tiene un solo panel");
                    return;
                }
                let id = ws.focused;
                if let Some(p) = ws.take_pane(id) {
                    let at = w.active + 1;
                    w.insert_tab(at, Workspace::new(p));
                }
            }
            Cmd::OpenSettings => {
                self.settings_open = Some(win_id);
                self.settings_state.capturing = None;
            }
            Cmd::OpenUpdates => {
                self.settings_open = Some(win_id);
                self.settings_state.capturing = None;
                self.settings_state.section = crate::settings_ui::Section::Actualizaciones;
            }
            Cmd::CheckUpdates => {
                self.settings_open = Some(win_id);
                self.settings_state.section = crate::settings_ui::Section::Actualizaciones;
                self.start_check(ctx, true);
            }
            Cmd::ApplyUpdate => self.apply_update(ctx),
            Cmd::Search => {
                let w = &mut self.windows[wi];
                w.popup = None;
                if let Some(p) = w.ws_mut().and_then(|ws| ws.focused_pane_mut()) {
                    p.open_search();
                }
            }
            Cmd::PromptPrev | Cmd::PromptNext => {
                if let Some(p) = self.windows[wi]
                    .ws_mut()
                    .and_then(|ws| ws.focused_pane_mut())
                {
                    p.jump_prompt(cmd == Cmd::PromptPrev);
                }
            }
            Cmd::CopySelection(id) => {
                let mut copied = false;
                if let Some(p) = self.pane_mut(id) {
                    let text = match p.vsel.take().filter(|v| v.has_selection()) {
                        Some(v) => v.text(),
                        None => p.backend.selectable_content(),
                    };
                    if !text.trim().is_empty() {
                        ctx.copy_text(text);
                        copied = true;
                    }
                }
                if copied {
                    self.windows[wi].toast("Copiado");
                }
            }
            Cmd::Toast(m) => self.windows[wi].toast(m),
            Cmd::Paste(id) => {
                let mut cb = arboard::Clipboard::new();
                let files = cb
                    .as_mut()
                    .ok()
                    .and_then(|c| c.get().file_list().ok())
                    .filter(|f| !f.is_empty());
                let text = cb.as_mut().ok().and_then(|c| c.get_text().ok());
                if std::env::var_os("NEON_DEBUG").is_some() {
                    eprintln!("[neon] pegar: {text:?}");
                }
                let has_image = cb.as_mut().ok().is_some_and(|c| c.get_image().is_ok());
                let mut msg = None;
                let mut confirm = None;
                let safe_paste = self.settings.safe_paste;
                if let Some(p) = self.pane_mut(id) {
                    if let Some(f) = files {
                        let s = p.format_paths(&f);
                        p.paste(&s);
                    } else if let Some(t) = text.filter(|t| !t.is_empty()) {
                        let check = safe_paste && !p.is_claude() && !p.is_alt_screen();
                        let why = if check {
                            crate::safepaste::reasons(&t, p.paste_runs_lines())
                        } else {
                            vec![]
                        };
                        if why.is_empty() {
                            p.paste(&t);
                        } else {
                            confirm = Some((id, t, why));
                        }
                    } else if has_image && p.is_claude() {
                        // Claude Code recibe la imagen del portapapeles con Alt+V.
                        p.insert("\x1bv");
                    } else if has_image {
                        msg =
                            Some("El portapapeles tiene una imagen; pégala dentro de Claude Code");
                    } else {
                        msg = Some("El portapapeles está vacío");
                    }
                }
                if let Some(m) = msg {
                    self.windows[wi].toast(m);
                }
                if confirm.is_some() {
                    self.windows[wi].paste_confirm = confirm;
                }
            }
            Cmd::Raw(id, text) => {
                if let Some(p) = self.pane_mut(id) {
                    p.insert(&text);
                }
            }
            Cmd::PastePaths(id, paths) => {
                if let Some(p) = self.pane_mut(id) {
                    let s = p.format_paths(&paths);
                    p.paste(&s);
                }
            }
            Cmd::ClearPane(id) => {
                if let Some(p) = self.pane_mut(id) {
                    p.clear_screen();
                }
            }
            Cmd::Insert(id, text) | Cmd::Run(id, text) => {
                let run = is_run;
                if let Some((pwi, ti)) = self.locate(id) {
                    let w = &mut self.windows[pwi];
                    w.active = ti;
                    let ws = &mut w.workspaces[ti];
                    ws.focused = id;
                    if let Some(p) = ws.pane_mut(id) {
                        if run {
                            p.run(&text);
                        } else {
                            p.replace_line(&text);
                        }
                    }
                }
            }
        }
    }

    /// Comandos corriendo en cualquier panel de la ventana.
    fn running_in_window(&self, wi: usize) -> Vec<String> {
        self.windows[wi]
            .workspaces
            .iter()
            .flat_map(|ws| {
                ws.pane_ids()
                    .into_iter()
                    .filter_map(|id| ws.pane(id)?.running())
            })
            .collect()
    }

    fn open_history(&mut self, wi: usize, mode: HistoryMode) {
        let w = &mut self.windows[wi];
        let Some(p) = w.ws().and_then(|ws| ws.focused_pane()) else {
            return;
        };
        let query = match mode {
            HistoryMode::Inline if p.typed_exact => p.typed.clone(),
            _ => String::new(),
        };
        let pane_id = p.id;
        w.palette = None;
        w.popup = Some(HistoryPopup {
            mode,
            pane_id,
            query,
            results: vec![],
            selected: 0,
            computed_for: None,
        });
    }

    // ───────────────────────── Arrastrar pestañas ─────────────────────────

    /// Posición global (pantalla) del puntero, vista desde la ventana `wi`.
    fn global_pointer(&self, ctx: &egui::Context, wi: usize) -> Option<Pos2> {
        if let Some((x, y)) = crate::window::global_cursor_px() {
            let ppp = ctx
                .input(|i| i.viewport().native_pixels_per_point)
                .unwrap_or(1.0);
            return Some(pos2(x / ppp, y / ppp));
        }
        let local = ctx.input(|i| i.pointer.latest_pos())?;
        Some(local + self.windows[wi].inner?.min.to_vec2())
    }

    /// Ventana bajo un punto global (prefiere la de origen si se superponen).
    fn window_at(&self, p: Pos2, prefer: u64) -> Option<usize> {
        let hits: Vec<usize> = (0..self.windows.len())
            .filter(|i| self.windows[*i].inner.is_some_and(|r| r.contains(p)))
            .collect();
        hits.iter()
            .copied()
            .find(|i| self.windows[*i].id == prefer)
            .or(hits.first().copied())
    }

    /// Qué pasaría al soltar en `p`.
    fn drop_target(&self, d: &TabDrag) -> DropTarget {
        let Some(ti) = self.window_at(d.global, d.win) else {
            return DropTarget::NewWindow;
        };
        let w = &self.windows[ti];
        let Some(inner) = w.inner else {
            return DropTarget::None;
        };
        let local = d.global - inner.min.to_vec2();
        if w.tabbar.contains(local) {
            let idx = w
                .tab_rects
                .iter()
                .filter(|r| r.center().x < local.x)
                .count();
            return DropTarget::TabBar {
                win: ti,
                index: idx,
            };
        }
        let same_tab = w.id == d.win && w.active == d.tab;
        if !same_tab {
            if let Some((pane, rect)) = w.pane_rects.iter().find(|(_, r)| r.contains(local)) {
                return DropTarget::Pane {
                    win: ti,
                    pane: *pane,
                    zone: DropZone::at(*rect, local),
                    rect: *rect,
                };
            }
        }
        DropTarget::None
    }

    fn finish_drag(&mut self, ctx: &egui::Context) {
        let Some(d) = self.drag.take() else { return };
        if !d.active {
            return;
        }
        let Some(src) = self.win_index(d.win) else {
            return;
        };
        match self.drop_target(&d) {
            DropTarget::None => {}
            DropTarget::TabBar { win, index } => {
                let mut index = index;
                if win == src && index > d.tab {
                    index -= 1;
                }
                if win == src && index == d.tab {
                    return;
                }
                if let Some(ws) = self.windows[src].remove_tab(d.tab) {
                    self.windows[win].insert_tab(index, ws);
                    ctx.send_viewport_cmd_to(
                        self.windows[win].viewport,
                        egui::ViewportCommand::Focus,
                    );
                    self.after_tab_removed(ctx, src);
                }
            }
            DropTarget::Pane {
                win, pane, zone, ..
            } => {
                let target_id = self.windows[win].id;
                let Some(ws) = self.windows[src].remove_tab(d.tab) else {
                    return;
                };
                let focused = ws.focused;
                if let Some(node) = ws.into_node() {
                    let Some(twi) = self.win_index(target_id) else {
                        return;
                    };
                    let (dir, before) = zone.split();
                    if let Some(tws) = self.windows[twi].ws_mut() {
                        if tws.insert_next_to(pane, node, dir, before) {
                            tws.focused = focused;
                        }
                    }
                    ctx.send_viewport_cmd_to(
                        self.windows[twi].viewport,
                        egui::ViewportCommand::Focus,
                    );
                }
                if let Some(s) = self.win_index(d.win) {
                    self.after_tab_removed(ctx, s);
                }
            }
            DropTarget::NewWindow => {
                let w = &self.windows[src];
                let size = w.inner.map(|r| r.size()).unwrap_or(vec2(1000.0, 680.0));
                let pos = Self::on_screen(ctx, d.global - vec2(90.0, 20.0), size);
                if w.workspaces.len() == 1 {
                    // Única pestaña: se mueve la ventana entera.
                    ctx.send_viewport_cmd_to(w.viewport, egui::ViewportCommand::OuterPosition(pos));
                } else if let Some(ws) = self.windows[src].remove_tab(d.tab) {
                    self.new_window(Some(pos), size, vec![ws]);
                }
            }
        }
    }

    // ───────────────────────── Una ventana ─────────────────────────

    fn window_ui(&mut self, ctx: &egui::Context, wi: usize) {
        let win_id = self.windows[wi].id;
        // Geometría y estado de la ventana.
        {
            let (inner, outer, focused, maximized, close) = ctx.input(|i| {
                let v = i.viewport();
                (
                    v.inner_rect,
                    v.outer_rect,
                    v.focused.unwrap_or(false),
                    v.maximized.unwrap_or(false),
                    v.close_requested(),
                )
            });
            let w = &mut self.windows[wi];
            w.inner = inner.or(w.inner);
            w.outer = outer.or(w.outer);
            w.focused = focused;
            w.maximized = maximized;
            if close && !self.allow_close {
                let running = self.running_in_window(wi);
                if wi == 0 && self.windows.len() == 1 {
                    // Cerrar la app: si hay algo corriendo, primero preguntar.
                    if self.settings.confirm_close && !running.is_empty() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                        self.pending.push((win_id, Cmd::CloseWindow));
                    }
                } else {
                    // Otras ventanas abiertas: se maneja sin cerrar la app.
                    ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                    self.pending.push((win_id, Cmd::CloseWindow));
                }
            }
        }
        if self.windows[wi].workspaces.is_empty() {
            return;
        }

        // Arrastre en curso que empezó en esta ventana (sin API nativa del mouse).
        if let Some(d) = &mut self.drag {
            if d.win == win_id && crate::window::left_button_down().is_none() {
                if let Some(local) = ctx.input(|i| i.pointer.latest_pos()) {
                    if let Some(inner) = self.windows[wi].inner {
                        d.global = local + inner.min.to_vec2();
                    }
                }
                if ctx.input(|i| i.pointer.any_released() || !i.pointer.primary_down()) {
                    d.released = true;
                }
            }
        }

        let mut cmds: Vec<Cmd> = vec![];
        self.shortcuts(ctx, wi, &mut cmds);
        cmds.extend(self.history_ui(ctx, wi));
        cmds.extend(self.dirs_ui(ctx, wi));
        self.update_ghost(wi);
        cmds.extend(self.palette_ui(ctx, wi));
        self.top_bar(ctx, wi, &mut cmds);

        if wi == 0 {
            if let Some(err) = self.error.clone() {
                egui::TopBottomPanel::bottom("error")
                    .frame(
                        Frame::new()
                            .fill(pal().red.gamma_multiply(0.25))
                            .inner_margin(Margin::same(6)),
                    )
                    .show(ctx, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(err.as_str());
                            if ui.small_button("Cerrar").clicked() {
                                self.error = None;
                            }
                        });
                    });
            }
        }

        let switch = {
            let w = &self.windows[wi];
            let cur = w.active.min(w.workspaces.len() - 1);
            let id = egui::Id::new(("tabsw", win_id));
            let (last, since): (usize, Instant) = ctx
                .data_mut(|d| d.get_temp(id))
                .unwrap_or((cur, Instant::now() - Duration::from_secs(9)));
            let since = if last != cur { Instant::now() } else { since };
            ctx.data_mut(|d| d.insert_temp(id, (cur, since)));
            (since.elapsed().as_secs_f32() / 0.16).min(1.0)
        };
        let dragging = self.drag.as_ref().is_some_and(|d| d.active);
        let opacity = self.opacity();
        let input_enabled = {
            let w = &self.windows[wi];
            w.popup.is_none()
                && w.dirs.is_none()
                && w.palette.is_none()
                && w.renaming.is_none()
                && w.confirm.is_none()
                && w.naming_layout.is_none()
                && w.paste_confirm.is_none()
                && (wi != 0 || (self.crash_report.is_none() && self.help.is_none()))
                && self.ports.as_ref().is_none_or(|(pw, _, _)| *pw != wi)
                && self.clip_popup.is_none_or(|(cw, _)| cw != wi)
                && self.finder.as_ref().is_none_or(|(fw, _)| *fw != wi)
                && self.settings_open.is_none()
                && !dragging
        };
        let w = &mut self.windows[wi];
        let active = w.active.min(w.workspaces.len() - 1);
        w.active = active;
        let ws = &mut w.workspaces[active];
        let mut behavior = PaneBehavior {
            focused: ws.focused,
            input_enabled,
            theme: &self.term_theme,
            font: &self.term_font,
            cmds: vec![],
            rects: vec![],
            opacity,
            copy_on_select: self.settings.copy_on_select,
            middle_paste: self.settings.middle_paste,
            blocks: self.settings.blocks,
            switch,
        };
        egui::CentralPanel::default()
            .frame(Frame::new().fill(pal().bg).inner_margin(Margin {
                left: 10,
                right: 10,
                top: 2,
                bottom: 10,
            }))
            .show(ctx, |ui| {
                let zoomed = ws
                    .zoomed
                    .and_then(|id| workspace::find_tile(&ws.tree.tiles, id));
                match zoomed {
                    Some(tile) => {
                        if let Some(egui_tiles::Tile::Pane(p)) = ws.tree.tiles.get_mut(tile) {
                            use egui_tiles::Behavior;
                            let _ = behavior.pane_ui(ui, tile, p);
                        }
                        let r = ui.max_rect();
                        ui.painter().text(
                            pos2(r.max.x - 14.0, r.min.y + 12.0),
                            Align2::RIGHT_TOP,
                            "Panel maximizado · Ctrl+Shift+Enter",
                            FontId::proportional(11.5),
                            pal().muted,
                        );
                    }
                    None => {
                        ws.zoomed = None;
                        ws.tree.ui(&mut behavior, ui);
                    }
                }
            });
        w.pane_rects = std::mem::take(&mut behavior.rects);
        cmds.extend(behavior.cmds);

        self.drag_feedback(ctx, wi);
        self.toast_ui(ctx, wi);
        self.confirm_ui(ctx, wi, &mut cmds);
        self.naming_ui(ctx, wi, &mut cmds);
        self.ports_ui(ctx, wi);
        self.clip_ui(ctx, wi, &mut cmds);
        self.finder_ui(ctx, wi);
        self.paste_confirm_ui(ctx, wi);
        if wi == 0 {
            self.crash_ui(ctx);
            // Primer arranque (o recorrido nuevo): bienvenida, después del logo.
            if self.help.is_none()
                && self.intro.is_none()
                && self.crash_report.is_none()
                && self.settings.tour_seen < crate::help::TOUR_VERSION
                && std::env::var_os("NEON_NO_INTRO").is_none()
            {
                self.help = Some(crate::help::Help::Tour(0));
                self.settings.tour_seen = crate::help::TOUR_VERSION;
                settings::set(self.settings.clone());
                self.settings_mtime = settings::save(&self.settings);
            }
            if let Some(h) = self.help.as_mut() {
                let quake = self.settings.quake_key.clone();
                if !crate::help::show(ctx, h, &quake, &mut cmds) {
                    self.help = None;
                }
            }
        }
        if self.settings_open == Some(win_id) {
            let out = crate::settings_ui::show(
                ctx,
                &mut self.settings,
                &mut self.settings_state,
                &self.profiles,
                &self.update,
            );
            if out.changed {
                self.settings_changed(ctx);
            }
            match out.update {
                Some(crate::settings_ui::UpdateAction::Check) => self.start_check(ctx, true),
                Some(crate::settings_ui::UpdateAction::Apply) => self.apply_update(ctx),
                None => {}
            }
            if out.close {
                self.settings_open = None;
            }
        }

        ui::resize_edges(ctx);
        if !crate::window::SYSTEM_DRAWS_BORDER && !self.windows[wi].maximized {
            ctx.layer_painter(egui::LayerId::new(
                egui::Order::Foreground,
                egui::Id::new("window_border"),
            ))
            .rect_stroke(
                ctx.screen_rect(),
                0.0,
                Stroke::new(1.0_f32, pal().border),
                StrokeKind::Inside,
            );
        }
        // Lo que se copió en esta ventana va al historial del portapapeles.
        let copied: Vec<String> = ctx.output(|o| {
            o.commands
                .iter()
                .filter_map(|c| match c {
                    egui::OutputCommand::CopyText(t) => Some(t.clone()),
                    _ => None,
                })
                .collect()
        });
        for t in copied {
            self.remember_clip(t);
        }
        for c in cmds {
            self.pending.push((win_id, c));
        }
    }

    /// Aviso de que Neon se cerró inesperadamente la vez anterior.
    fn crash_ui(&mut self, ctx: &egui::Context) {
        let Some(report) = self.crash_report.clone() else { return };
        let p = pal();
        let mut close = ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape));
        let modal = egui::Modal::new(egui::Id::new("crash_report"))
            .frame(
                Frame::new()
                    .fill(p.surface)
                    .stroke(Stroke::new(1.0_f32, p.border))
                    .corner_radius(CornerRadius::same(12))
                    .inner_margin(Margin::same(18)),
            )
            .show(ctx, |ui| {
                ui.set_width(560.0);
                ui.label(RichText::new("Neon se cerró inesperadamente la última vez").size(16.0).strong());
                ui.add_space(4.0);
                ui.label(
                    RichText::new(
                        "Quedó un reporte de lo que pasó. Si lo mandas, ayuda a arreglarlo. No se envía nada solo: tú decides.",
                    )
                    .color(p.muted),
                );
                ui.add_space(8.0);
                Frame::new()
                    .fill(p.bg)
                    .corner_radius(CornerRadius::same(8))
                    .inner_margin(Margin::same(10))
                    .show(ui, |ui| {
                        egui::ScrollArea::vertical().max_height(200.0).show(ui, |ui| {
                            let head: String = report.lines().take(8).collect::<Vec<_>>().join("\n");
                            ui.label(RichText::new(head).monospace().size(12.0).color(p.text));
                        });
                    });
                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    if ui.button("Copiar reporte").clicked() {
                        ctx.copy_text(report.clone());
                    }
                    if ui
                        .button("Reportar en GitHub")
                        .on_hover_text("Copia el reporte y abre un issue: pégalo ahí")
                        .clicked()
                    {
                        ctx.copy_text(report.clone());
                        let _ = open::that(crate::crash::issue_url(&report));
                    }
                    if ui.button("Abrir carpeta").clicked() {
                        let _ = open::that(crate::crash::folder());
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui.button("Cerrar").clicked() {
                            close = true;
                        }
                    });
                });
            });
        if modal.should_close() {
            close = true;
        }
        if close {
            self.crash_report = None;
        }
    }

    /// Pegado seguro: muestra qué se va a pegar y por qué conviene mirarlo.
    fn paste_confirm_ui(&mut self, ctx: &egui::Context, wi: usize) {
        let w = &mut self.windows[wi];
        let Some((pid, text, why)) = w.paste_confirm.clone() else { return };
        let p = pal();
        let hidden = crate::safepaste::has_hidden(&text);
        let esc = ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape));
        let mut choice: Option<Option<String>> = if esc { Some(None) } else { None };
        let modal = egui::Modal::new(egui::Id::new(("paste_confirm", w.id)))
            .frame(
                Frame::new()
                    .fill(p.surface)
                    .stroke(Stroke::new(1.0_f32, p.border))
                    .corner_radius(CornerRadius::same(12))
                    .inner_margin(Margin::same(18)),
            )
            .show(ctx, |ui| {
                ui.set_width(560.0);
                ui.label(RichText::new("¿Pegar esto?").size(16.0).strong());
                ui.add_space(6.0);
                for r in &why {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(RichText::new("⚠").color(p.yellow));
                        ui.label(RichText::new(r).color(p.text));
                    });
                }
                ui.add_space(8.0);
                // Vista previa con lo invisible a la vista.
                let preview: String = text
                    .lines()
                    .take(14)
                    .map(|l| {
                        l.chars()
                            .map(|c| {
                                if crate::safepaste::has_hidden(&c.to_string()) {
                                    format!("⟨U+{:04X}⟩", c as u32)
                                } else {
                                    c.to_string()
                                }
                            })
                            .collect::<String>()
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
                let more = text.lines().count().saturating_sub(14);
                Frame::new()
                    .fill(p.bg)
                    .corner_radius(CornerRadius::same(8))
                    .inner_margin(Margin::same(10))
                    .show(ui, |ui| {
                        egui::ScrollArea::vertical().max_height(240.0).show(ui, |ui| {
                            ui.label(RichText::new(preview).monospace().size(12.5).color(p.text));
                            if more > 0 {
                                ui.label(RichText::new(format!("… y {more} líneas más")).color(p.muted).size(11.5));
                            }
                        });
                    });
                ui.add_space(12.0);
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let go = egui::Button::new(RichText::new("Pegar igual").color(egui::Color32::WHITE))
                        .fill(p.red);
                    if ui.add(go).clicked() {
                        choice = Some(Some(text.clone()));
                    }
                    if hidden && ui.button("Pegar sin lo invisible").clicked() {
                        choice = Some(Some(crate::safepaste::clean(&text)));
                    }
                    if ui.button("Cancelar").clicked() {
                        choice = Some(None);
                    }
                });
            });
        if modal.should_close() && choice.is_none() {
            choice = Some(None);
        }
        if let Some(c) = choice {
            self.windows[wi].paste_confirm = None;
            if let Some(t) = c {
                if let Some(pane) = self.pane_mut(pid) {
                    pane.paste(&t);
                }
            }
        }
    }

    /// Buscador de archivos: escribir filtra, Enter abre en el editor,
    /// Tab pega la ruta en la terminal, Esc cierra.
    fn finder_ui(&mut self, ctx: &egui::Context, wi: usize) {
        let Some((fw, f)) = self.finder.as_mut() else { return };
        if *fw != wi {
            return;
        }
        let (up, down, enter, tab, esc) = ctx.input_mut(|i| {
            (
                i.consume_key(Modifiers::NONE, Key::ArrowUp),
                i.consume_key(Modifiers::NONE, Key::ArrowDown),
                i.consume_key(Modifiers::NONE, Key::Enter),
                i.consume_key(Modifiers::NONE, Key::Tab),
                i.consume_key(Modifiers::NONE, Key::Escape),
            )
        });
        let n = f.results.len();
        if up {
            f.selected = f.selected.saturating_sub(1);
        }
        if down {
            f.selected = (f.selected + 1).min(n.saturating_sub(1));
        }
        let p = pal();
        let mut query = f.query.clone();
        let mut close = esc;
        let mut open = (enter && n > 0).then_some(f.selected);
        let paste = (tab && n > 0).then_some(f.selected);
        let modal = egui::Modal::new(egui::Id::new(("finder", wi)))
            .frame(
                Frame::new()
                    .fill(p.surface)
                    .stroke(Stroke::new(1.0_f32, p.border))
                    .corner_radius(CornerRadius::same(12))
                    .inner_margin(Margin::same(14)),
            )
            .show(ctx, |ui| {
                ui.set_width(620.0);
                let edit = ui.add(
                    egui::TextEdit::singleline(&mut query)
                        .hint_text("Buscar archivo… (por ejemplo «apprs» encuentra src/app.rs)")
                        .font(FontId::proportional(15.0))
                        .desired_width(f32::INFINITY),
                );
                edit.request_focus();
                ui.add_space(4.0);
                ui.label(
                    RichText::new(format!(
                        "{} · {} archivos{}  ·  Enter abre · Tab pega la ruta · Esc cierra",
                        ui::short_path(&f.root.display().to_string()),
                        f.count(),
                        if f.truncated { " (solo los primeros)" } else { "" }
                    ))
                    .color(p.muted)
                    .size(11.0),
                );
                ui.add_space(6.0);
                if n == 0 {
                    ui.label(RichText::new("Nada coincide").color(p.muted));
                }
                egui::ScrollArea::vertical().max_height(400.0).show(ui, |ui| {
                    for i in 0..n.min(60) {
                        let path = f.path(i);
                        let (dir, name) = match path.rfind(['/', '\\']) {
                            Some(k) => (&path[..=k], &path[k + 1..]),
                            None => ("", path),
                        };
                        let (rect, resp) =
                            ui.allocate_exact_size(vec2(ui.available_width(), 26.0), Sense::click());
                        if i == f.selected {
                            ui.painter().rect_filled(rect, CornerRadius::same(6), theme::mix(p.surface, p.red, 0.16));
                            ui.scroll_to_rect(rect, None);
                        } else if resp.hovered() {
                            ui.painter().rect_filled(rect, CornerRadius::same(6), p.hover);
                        }
                        let mut job = egui::text::LayoutJob::default();
                        job.append(name, 0.0, egui::TextFormat::simple(FontId::monospace(13.0), p.text));
                        job.append(
                            &format!("   {dir}"),
                            0.0,
                            egui::TextFormat::simple(FontId::monospace(11.5), p.muted),
                        );
                        let galley = ui.fonts(|fo| fo.layout_job(job));
                        ui.painter().galley(
                            pos2(rect.min.x + 10.0, rect.center().y - galley.size().y / 2.0),
                            galley,
                            p.text,
                        );
                        if resp.clicked() {
                            open = Some(i);
                        }
                    }
                });
            });
        if modal.should_close() {
            close = true;
        }
        f.set_query(&query);
        let mut toast = None;
        if let Some(i) = open.filter(|i| *i < f.results.len()) {
            let full = f.full(i).display().to_string();
            let pid = self.windows[wi].ws().map(|ws| ws.focused);
            if let Some(p) = pid.and_then(|id| self.pane_mut(id)) {
                toast = p.open_link(&full);
            }
            close = true;
        } else if let Some(i) = paste.filter(|i| *i < f.results.len()) {
            let full = f.full(i);
            let pid = self.windows[wi].ws().map(|ws| ws.focused);
            // Relativa a la carpeta de la shell si queda adentro (más corta).
            let cwd = pid
                .and_then(|id| self.pane_mut(id))
                .map(|p| std::path::PathBuf::from(&p.cwd));
            let shown = cwd
                .and_then(|c| full.strip_prefix(&c).ok().map(|r| r.display().to_string()))
                .unwrap_or_else(|| full.display().to_string());
            let text = if shown.contains(' ') { format!("\"{shown}\"") } else { shown };
            if let Some(p) = pid.and_then(|id| self.pane_mut(id)) {
                p.insert(&text);
                p.typed.push_str(&text);
            }
            close = true;
        }
        if let Some(t) = toast {
            self.windows[wi].toast(t);
        }
        if close {
            // Se suelta la lista de archivos.
            self.finder = None;
        }
    }

    /// Guarda un texto copiado (sin repetir, el más nuevo primero, acotado).
    fn remember_clip(&mut self, t: String) {
        const CLIPS_MAX: usize = 20;
        const CLIP_BYTES: usize = 64 * 1024;
        if t.trim().is_empty() {
            return;
        }
        let mut t = t;
        if t.len() > CLIP_BYTES {
            let mut cut = CLIP_BYTES;
            while !t.is_char_boundary(cut) {
                cut -= 1;
            }
            t.truncate(cut);
        }
        self.clips.retain(|c| *c != t);
        self.clips.push_front(t);
        self.clips.truncate(CLIPS_MAX);
    }

    /// Historial del portapapeles: ↑↓ elegir, Enter pega, Esc cierra.
    fn clip_ui(&mut self, ctx: &egui::Context, wi: usize, cmds: &mut Vec<Cmd>) {
        let Some((cw, sel)) = self.clip_popup else { return };
        if cw != wi {
            return;
        }
        let n = self.clips.len();
        let (up, down, enter, esc) = ctx.input_mut(|i| {
            (
                i.consume_key(Modifiers::NONE, Key::ArrowUp),
                i.consume_key(Modifiers::NONE, Key::ArrowDown),
                i.consume_key(Modifiers::NONE, Key::Enter),
                i.consume_key(Modifiers::NONE, Key::Escape),
            )
        });
        // Un número del 1 al 9 elige directo.
        let digit = ctx.input(|i| {
            i.events.iter().find_map(|e| match e {
                egui::Event::Text(t) => t.chars().next().and_then(|c| c.to_digit(10)),
                _ => None,
            })
        });
        let mut sel = sel.min(n.saturating_sub(1));
        if up {
            sel = sel.saturating_sub(1);
        }
        if down {
            sel = (sel + 1).min(n.saturating_sub(1));
        }
        let mut chosen = (enter && n > 0).then_some(sel);
        if let Some(d) = digit.filter(|d| (1..=9).contains(d) && (*d as usize) <= n) {
            chosen = Some(d as usize - 1);
        }
        let p = pal();
        let mut close = esc;
        let modal = egui::Modal::new(egui::Id::new(("clips", wi)))
            .frame(
                Frame::new()
                    .fill(p.surface)
                    .stroke(Stroke::new(1.0_f32, p.border))
                    .corner_radius(CornerRadius::same(12))
                    .inner_margin(Margin::same(16)),
            )
            .show(ctx, |ui| {
                ui.set_width(560.0);
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Portapapeles").size(16.0).strong());
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.label(RichText::new("1–9 o Enter pega · Esc cierra").color(p.muted).size(11.0));
                    });
                });
                ui.add_space(8.0);
                if n == 0 {
                    ui.label(RichText::new("Todavía no copiaste nada en Neon.").color(p.muted));
                }
                egui::ScrollArea::vertical().max_height(420.0).show(ui, |ui| {
                    for (i, c) in self.clips.iter().enumerate() {
                        let first = c.lines().find(|l| !l.trim().is_empty()).unwrap_or("").trim();
                        let lines = c.lines().count();
                        let label = if lines > 1 {
                            format!("{}   (+{} líneas)", ui::truncate(first, 70), lines - 1)
                        } else {
                            ui::truncate(first, 84)
                        };
                        let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 28.0), Sense::click());
                        if i == sel {
                            ui.painter().rect_filled(rect, CornerRadius::same(6), theme::mix(p.surface, p.red, 0.16));
                        } else if resp.hovered() {
                            ui.painter().rect_filled(rect, CornerRadius::same(6), p.hover);
                        }
                        if i < 9 {
                            ui.painter().text(
                                pos2(rect.min.x + 12.0, rect.center().y),
                                Align2::LEFT_CENTER,
                                format!("{}", i + 1),
                                FontId::monospace(12.0),
                                p.muted,
                            );
                        }
                        ui.painter().text(
                            pos2(rect.min.x + 32.0, rect.center().y),
                            Align2::LEFT_CENTER,
                            label,
                            FontId::monospace(12.5),
                            p.text,
                        );
                        if resp.clicked() {
                            chosen = Some(i);
                        }
                    }
                });
            });
        if modal.should_close() {
            close = true;
        }
        if let Some(i) = chosen {
            if let (Some(text), Some(pid)) = (
                self.clips.get(i).cloned(),
                self.windows[wi].ws().map(|ws| ws.focused),
            ) {
                if let Some(pane) = self.pane_mut(pid) {
                    pane.paste(&text);
                }
                // Queda arriba y también en el portapapeles del sistema.
                ctx.copy_text(text);
            }
            close = true;
        }
        self.clip_popup = if close { None } else { Some((wi, sel)) };
        let _ = cmds;
    }

    fn shortcuts(&mut self, ctx: &egui::Context, wi: usize, cmds: &mut Vec<Cmd>) {
        let w = &self.windows[wi];
        if w.palette.is_some()
            || w.renaming.is_some()
            || w.confirm.is_some()
            || self.settings_open.is_some()
        {
            return;
        }
        let pressed = |m: Modifiers, k: Key| ctx.input_mut(|i| i.consume_key(m, k));
        // Claude Code: Shift+Enter, Ctrl+V inteligente y archivos soltados.
        {
            let w = &self.windows[wi];
            let a = w.active.min(w.workspaces.len().saturating_sub(1));
            if let Some(ws) = w.workspaces.get(a) {
                let fid = ws.focused;
                let claude = ws.focused_pane().is_some_and(|p| p.is_claude());
                if self.settings.shift_enter_newline
                    && claude
                    && pressed(Modifiers::SHIFT, Key::Enter)
                {
                    cmds.push(Cmd::Raw(fid, "\n".into()));
                }
                // Con una selección propia (Claude Code), Ctrl+C la copia en vez de interrumpir.
                let vsel_ready = ws
                    .focused_pane()
                    .and_then(|p| p.vsel.as_ref())
                    .is_some_and(|v| v.has_selection() && !v.dragging);
                if vsel_ready
                    && ctx.input(|i| i.events.iter().any(|e| matches!(e, egui::Event::Copy)))
                {
                    ctx.input_mut(|i| i.events.retain(|e| !matches!(e, egui::Event::Copy)));
                    cmds.push(Cmd::CopySelection(fid));
                }
                if self.settings.smart_paste {
                    // egui convierte Ctrl+V / Ctrl+Shift+V en un evento de pegado (no en una
                    // tecla); si el portapapeles solo tiene una imagen no llega ningún evento,
                    // así que en Windows se mira además el estado de la tecla V.
                    let paste_ev =
                        ctx.input(|i| i.events.iter().any(|e| matches!(e, egui::Event::Paste(_))));
                    let vid = egui::Id::new(("v_was_down", wi));
                    let v_now = crate::window::v_key_down().unwrap_or(false)
                        && ctx.input(|i| i.modifiers.ctrl && i.focused);
                    let v_was: bool = ctx.data(|d| d.get_temp(vid)).unwrap_or(false);
                    ctx.data_mut(|d| d.insert_temp(vid, v_now));
                    let image_only = v_now && !v_was && !paste_ev;
                    if paste_ev || image_only || pressed(Modifiers::CTRL, Key::V) {
                        ctx.input_mut(|i| i.events.retain(|e| !matches!(e, egui::Event::Paste(_))));
                        cmds.push(Cmd::Paste(fid));
                    }
                }
                let dropped: Vec<std::path::PathBuf> = ctx.input(|i| {
                    i.raw
                        .dropped_files
                        .iter()
                        .filter_map(|f| f.path.clone())
                        .collect()
                });
                if !dropped.is_empty() {
                    cmds.push(Cmd::FocusPane(fid));
                    cmds.push(Cmd::PastePaths(fid, dropped));
                }
            }
        }
        // Atajos configurables (Ajustes → Atajos).
        for (sc, c) in &self.keymap {
            if ctx.input_mut(|i| i.consume_shortcut(sc)) {
                cmds.push(c.clone());
            }
        }
        // Variantes que siempre funcionan.
        if pressed(Modifiers::CTRL, Key::Plus) {
            cmds.push(Cmd::FontBigger);
        }
        let nums = [
            Key::Num1,
            Key::Num2,
            Key::Num3,
            Key::Num4,
            Key::Num5,
            Key::Num6,
            Key::Num7,
            Key::Num8,
            Key::Num9,
        ];
        for (i, k) in nums.iter().enumerate() {
            if pressed(Modifiers::CTRL, *k) {
                // Ctrl+9 = última pestaña, como en el navegador.
                let n = self.windows[wi].workspaces.len();
                cmds.push(Cmd::SelectTab(if i == 8 { n.saturating_sub(1) } else { i }));
            }
        }

        if self.windows[wi].popup.is_some() || self.windows[wi].dirs.is_some() {
            return;
        }
        // «# pedido» + Enter: Claude escribe el comando (no se ejecuta solo).
        if self.settings.ai {
            let req = self.windows[wi]
                .ws()
                .and_then(|ws| ws.focused_pane())
                .filter(|p| p.at_prompt && p.typed_exact && p.ai_busy.is_none() && !p.is_alt_screen())
                .and_then(|p| {
                    let t = p.typed.trim_start();
                    let r = t.strip_prefix('#')?.trim();
                    (r.chars().count() >= 3).then(|| (p.id, r.to_string(), p.cwd.clone(), p.profile))
                });
            if let Some((pid, request, cwd, profile)) = req {
                if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Enter)) {
                    let system = crate::ai::command_system(shell_label(profile), os_label(), &cwd);
                    self.ai_jobs.push(crate::ai::spawn(
                        pid,
                        crate::ai::AiKind::Command { request: request.clone() },
                        system,
                        request,
                        cwd,
                        self.settings.ai_model.clone(),
                        ctx.clone(),
                    ));
                    if let Some(p) = self.pane_mut(pid) {
                        p.ai_busy = Some(Instant::now());
                    }
                    return;
                }
            }
            // Esc mientras Claude piensa: cancelar.
            let busy = self.windows[wi]
                .ws()
                .and_then(|ws| ws.focused_pane())
                .filter(|p| p.ai_busy.is_some())
                .map(|p| p.id);
            if let Some(pid) = busy {
                if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape)) {
                    self.ai_jobs.retain(|j| {
                        !(j.pane_id == pid && matches!(j.kind, crate::ai::AiKind::Command { .. }))
                    });
                    if let Some(p) = self.pane_mut(pid) {
                        p.ai_busy = None;
                    }
                }
            }
        }
        // ↑ en el prompt abre el historial pegado al cursor. Si hay un programa
        // corriendo (vim, ssh, un servidor…) la flecha le llega como siempre.
        let can_inline = self.windows[wi]
            .ws()
            .and_then(|ws| ws.focused_pane())
            .is_some_and(|p| {
                p.at_prompt && !p.is_alt_screen() && p.profile.has_integration() && !p.search.open
            });
        if can_inline && pressed(Modifiers::NONE, Key::ArrowUp) {
            self.open_history(wi, HistoryMode::Inline);
            return;
        }
        // → al final de la línea acepta la sugerencia del historial.
        let accept = self.windows[wi]
            .ws()
            .and_then(|ws| ws.focused_pane())
            .and_then(|p| {
                let (t, rest) = p.ghost.as_ref()?;
                (*t == p.typed && p.cursor_tail_blank()).then(|| rest.clone())
            });
        if let Some(rest) = accept {
            if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::ArrowRight)) {
                if let Some(p) = self.windows[wi].ws_mut().and_then(|ws| ws.focused_pane_mut()) {
                    p.insert(&rest);
                    p.typed.push_str(&rest);
                    p.ghost = None;
                }
            }
        }
        // Seguir lo que se escribe para saber qué hay en la línea.
        let events = ctx.input(|i| i.events.clone());
        if let Some(p) = self.windows[wi]
            .ws_mut()
            .and_then(|ws| ws.focused_pane_mut())
        {
            p.track_key_events(&events);
        }
    }

    /// Barra superior: logo, pestañas (arrastrables), + con perfiles y botones de ventana.
    fn top_bar(&mut self, ctx: &egui::Context, wi: usize, cmds: &mut Vec<Cmd>) {
        let win_id = self.windows[wi].id;
        let maximized = self.windows[wi].maximized;
        let drag_src = self
            .drag
            .as_ref()
            .filter(|d| d.active && d.win == win_id)
            .map(|d| d.tab);
        let mut start_drag: Option<(usize, Pos2)> = None;
        let mut tab_rects = vec![];
        let mut tabbar = Rect::NOTHING;
        let profiles: Vec<(ProfileKind, &'static str)> =
            self.profiles.iter().map(|p| (p.kind, p.name)).collect();
        let bg: Vec<(u64, String, BgState)> = self
            .background
            .iter()
            .map(|b| (b.pane.id, b.label.clone(), b.state))
            .collect();
        let new_version = match &self.update {
            UpdateState::Available(r) => Some(r.version.clone()),
            _ => None,
        };
        let logo = self.logo.clone();
        let bar_opacity = self.opacity();
        let w = &mut self.windows[wi];

        egui::TopBottomPanel::top(egui::Id::new(("tabs", win_id)))
            .exact_height(BAR_H)
            .frame(
                Frame::new()
                    .fill(pal().bg.gamma_multiply(bar_opacity))
                    .inner_margin(Margin {
                        left: 14,
                        right: 0,
                        top: 0,
                        bottom: 0,
                    }),
            )
            .show_separator_line(false)
            .show(ctx, |ui| {
                tabbar = ui.max_rect();
                ui.horizontal_centered(|ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    ui.add(egui::Image::new(&logo).fit_to_exact_size(vec2(20.0, 20.0)));
                    ui.add_space(8.0);
                    let mut commit_rename = None;
                    let titles: Vec<String> = w.workspaces.iter().map(|ws| ws.title()).collect();
                    // Hueco de la pestaña que se acaba de cerrar (se encoge).
                    let gap = w.closing_tab.and_then(|(at, width, t0)| {
                        let t = t0.elapsed().as_secs_f32() / ui::CLOSE_ANIM.as_secs_f32();
                        (t < 1.0).then_some((at, width, t))
                    });
                    if gap.is_none() {
                        w.closing_tab = None;
                    }
                    let draw_gap = |ui: &mut egui::Ui, i: usize| {
                        if let Some((at, width, t)) = gap.filter(|g| g.0 == i) {
                            let ease = 1.0 - (1.0 - t).powi(2);
                            let (rect, _) = ui.allocate_exact_size(
                                vec2((width - 4.0) * (1.0 - ease), 30.0),
                                Sense::hover(),
                            );
                            ui.painter().rect_filled(
                                rect.shrink2(vec2(0.0, 4.0 * ease)),
                                CornerRadius::same(8),
                                pal().surface.gamma_multiply(1.0 - ease),
                            );
                            let _ = at;
                            ui.ctx().request_repaint();
                        }
                    };
                    for i in 0..=w.workspaces.len() {
                        draw_gap(ui, i);
                        if i == w.workspaces.len() {
                            break;
                        }
                        // Renombrando: un campo de texto en lugar de la pestaña.
                        if let Some((ri, buf)) = &mut w.renaming {
                            if *ri == i {
                                let hint = w.workspaces[i]
                                    .focused_pane()
                                    .map(|p| p.display_title())
                                    .unwrap_or_default();
                                let r = ui.add(
                                    egui::TextEdit::singleline(buf)
                                        .hint_text(hint)
                                        .desired_width(160.0)
                                        .font(FontId::proportional(13.0))
                                        .margin(vec2(8.0, 6.0)),
                                );
                                r.request_focus();
                                tab_rects.push(r.rect);
                                let (enter, esc) = ui.input(|inp| {
                                    (inp.key_pressed(Key::Enter), inp.key_pressed(Key::Escape))
                                });
                                if esc {
                                    commit_rename = Some(None);
                                } else if enter || r.lost_focus() {
                                    commit_rename = Some(Some((i, buf.clone())));
                                }
                                continue;
                            }
                        }
                        let ws = &w.workspaces[i];
                        let color = ws
                            .color
                            .or(ws.auto_color)
                            .and_then(|c| tab_colors().get(c as usize).copied());
                        let title = ws.title();
                        let badge = if i == w.active {
                            ui::Badge::None
                        } else if ws.panes().any(|p| p.attention) {
                            ui::Badge::Attention
                        } else if ws.panes().any(|p| p.cmd_done == Some(false)) {
                            ui::Badge::Failed
                        } else if ws.panes().any(|p| p.is_working()) {
                            ui::Badge::Working
                        } else if ws.panes().any(|p| p.running().is_some() && !p.is_claude()) {
                            ui::Badge::Running
                        } else if ws.panes().any(|p| p.claude_done || p.cmd_done == Some(true)) {
                            ui::Badge::Done
                        } else if ws.panes().any(|p| p.activity) {
                            ui::Badge::Activity
                        } else {
                            ui::Badge::None
                        };
                        let r =
                            ui::tab(ui, &title, i == w.active, color, drag_src == Some(i), badge);
                        let r = match badge.hint() {
                            Some(h) => r.on_hover_text(h),
                            None => r,
                        };
                        tab_rects.push(r.rect);
                        if r.clicked() {
                            cmds.push(Cmd::SelectTab(i));
                        }
                        if r.double_clicked() {
                            cmds.push(Cmd::StartRename(i));
                        }
                        if r.middle_clicked() {
                            cmds.push(Cmd::CloseTab(i));
                        }
                        if r.drag_started_by(egui::PointerButton::Primary) {
                            let p = r.interact_pointer_pos().unwrap_or(r.rect.center());
                            start_drag = Some((i, p));
                        }
                        r.context_menu(|ui| {
                            ui.set_min_width(200.0);
                            if ui.button("Renombrar").clicked() {
                                cmds.push(Cmd::StartRename(i));
                                ui.close_menu();
                            }
                            ui.horizontal(|ui| {
                                for (ci, c) in tab_colors().iter().enumerate() {
                                    let (rect, resp) =
                                        ui.allocate_exact_size(vec2(18.0, 18.0), Sense::click());
                                    ui.painter().circle_filled(rect.center(), 6.5, *c);
                                    if resp.hovered() {
                                        ui.painter().circle_stroke(
                                            rect.center(),
                                            8.5,
                                            Stroke::new(1.0_f32, pal().text),
                                        );
                                    }
                                    if resp.clicked() {
                                        cmds.push(Cmd::SetColor(i, Some(ci as u8)));
                                        ui.close_menu();
                                    }
                                }
                                if ui.small_button("Sin color").clicked() {
                                    cmds.push(Cmd::SetColor(i, None));
                                    ui.close_menu();
                                }
                            });
                            ui.separator();
                            if titles.len() > 1 {
                                ui.menu_button("Dividir con…", |ui| {
                                    for (j, t) in titles.iter().enumerate() {
                                        if j == i {
                                            continue;
                                        }
                                        ui.horizontal(|ui| {
                                            let (r, _) = ui.allocate_exact_size(
                                                vec2(150.0, 22.0),
                                                Sense::hover(),
                                            );
                                            ui.painter().text(
                                                r.left_center(),
                                                egui::Align2::LEFT_CENTER,
                                                format!("{}  {}", j + 1, ui::truncate(t, 18)),
                                                egui::FontId::proportional(13.5),
                                                pal().text,
                                            );
                                            for (side, label, tip) in [
                                                (Dir::Right, "→ derecha", "Ponerla a la derecha"),
                                                (Dir::Down, "↓ abajo", "Ponerla abajo"),
                                            ] {
                                                if ui
                                                    .small_button(label)
                                                    .on_hover_text(tip)
                                                    .clicked()
                                                {
                                                    cmds.push(Cmd::JoinTab {
                                                        from: j,
                                                        into: i,
                                                        side,
                                                    });
                                                    ui.close_menu();
                                                }
                                            }
                                        });
                                    }
                                });
                            }
                            if ui.button("Mover a ventana nueva").clicked() {
                                cmds.push(Cmd::TabToNewWindow(i));
                                ui.close_menu();
                            }
                            if ui.button("Cerrar pestaña").clicked() {
                                cmds.push(Cmd::CloseTab(i));
                                ui.close_menu();
                            }
                        });
                    }
                    match commit_rename {
                        Some(Some((i, name))) => {
                            cmds.push(Cmd::Rename(i, name));
                            w.renaming = None;
                        }
                        Some(None) => w.renaming = None,
                        None => {}
                    }

                    // + abre la pestaña con el perfil predeterminado; ▾ elige otro.
                    if ui
                        .add(egui::Button::new(
                            RichText::new("+").size(16.0).color(pal().muted),
                        ))
                        .on_hover_text("Nueva pestaña  (Ctrl+Shift+T)")
                        .clicked()
                    {
                        cmds.push(Cmd::NewTab(None));
                    }
                    let (rect, chev) = ui.allocate_exact_size(vec2(18.0, 26.0), Sense::click());
                    if chev.hovered() {
                        ui.painter()
                            .rect_filled(rect, CornerRadius::same(6), pal().surface);
                    }
                    let c = rect.center();
                    let st = Stroke::new(
                        1.3_f32,
                        if chev.hovered() {
                            pal().text
                        } else {
                            pal().muted
                        },
                    );
                    ui.painter()
                        .line_segment([c + vec2(-3.5, -1.5), c + vec2(0.0, 2.0)], st);
                    ui.painter()
                        .line_segment([c + vec2(0.0, 2.0), c + vec2(3.5, -1.5)], st);
                    let menu_id = ui.make_persistent_id(("profiles", win_id));
                    if chev.on_hover_text("Elegir shell").clicked() {
                        ui.memory_mut(|m| m.toggle_popup(menu_id));
                    }
                    egui::popup::popup_below_widget(
                        ui,
                        menu_id,
                        &ui.interact(rect, menu_id.with("anchor"), Sense::hover()),
                        egui::PopupCloseBehavior::CloseOnClick,
                        |ui| {
                            ui.set_min_width(200.0);
                            ui.label(
                                RichText::new("Nueva pestaña con…")
                                    .size(11.5)
                                    .color(pal().muted),
                            );
                            for (kind, name) in &profiles {
                                if ui.button(*name).clicked() {
                                    cmds.push(Cmd::NewTab(Some(*kind)));
                                }
                            }
                        },
                    );

                    // Procesos en segundo plano: un chip por cada uno.
                    let mut bg_w = 0.0;
                    for (id, label, state) in &bg {
                        let (color, hint) = match state {
                            BgState::Running => (pal().green, "Corriendo"),
                            BgState::Done(true) => (pal().muted, "Terminó"),
                            BgState::Done(false) => (pal().red, "Falló"),
                            BgState::Exited => (pal().red, "Se cerró"),
                        };
                        let text = ui::truncate(label, 18);
                        let font = FontId::proportional(12.0);
                        let tw = ui.fonts(|f| f.layout_no_wrap(text.clone(), font.clone(), pal().text).size().x);
                        let (r, resp) = ui.allocate_exact_size(vec2(tw + 26.0, 24.0), Sense::click());
                        bg_w += r.width() + 4.0;
                        let fill = if resp.hovered() { pal().hover } else { pal().surface };
                        ui.painter().rect(r, CornerRadius::same(12), fill, Stroke::new(1.0_f32, pal().border), StrokeKind::Inside);
                        ui.painter().circle_filled(pos2(r.min.x + 11.0, r.center().y), 3.5, color);
                        ui.painter().text(pos2(r.min.x + 19.0, r.center().y), Align2::LEFT_CENTER, text, font, pal().text);
                        let resp = resp.on_hover_text(format!("{hint} en segundo plano · clic: traer de vuelta · clic derecho: detener"));
                        if resp.clicked() {
                            cmds.push(Cmd::BgRestore(*id));
                        }
                        resp.context_menu(|ui| {
                            if ui.button("Traer de vuelta").clicked() {
                                cmds.push(Cmd::BgRestore(*id));
                                ui.close_menu();
                            }
                            if ui.button("Detener").clicked() {
                                cmds.push(Cmd::BgStop(*id));
                                ui.close_menu();
                            }
                        });
                    }

                    // Espacio libre: arrastrar mueve la ventana.
                    let chip_w = (if new_version.is_some() { 96.0 } else { 0.0 }) + bg_w;
                    let free =
                        (ui.available_width() - 3.0 * WINDOW_BUTTON_W - 40.0 - chip_w).max(0.0);
                    let (_, dragbar) =
                        ui.allocate_exact_size(vec2(free, BAR_H), Sense::click_and_drag());
                    if dragbar.double_clicked() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!maximized));
                    } else if dragbar.drag_started_by(egui::PointerButton::Primary) {
                        ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
                    }

                    // Aviso de versión nueva.
                    if let Some(v) = &new_version {
                        let chip = ui
                            .add_sized(
                                vec2(88.0, 24.0),
                                egui::Button::new(
                                    RichText::new(format!("↑ v{v}"))
                                        .size(12.0)
                                        .color(Color32::WHITE),
                                )
                                .fill(pal().red)
                                .corner_radius(CornerRadius::same(12)),
                            )
                            .on_hover_text("Hay una versión nueva de Neon");
                        if chip.clicked() {
                            cmds.push(Cmd::OpenUpdates);
                        }
                        ui.add_space(8.0);
                    }

                    // Ajustes (ícono de engranaje de la Nerd Font).
                    let gear = ui
                        .add_sized(
                            vec2(36.0, 30.0),
                            egui::Button::new(
                                RichText::new("\u{f013}").size(15.0).color(pal().muted),
                            )
                            .frame(false),
                        )
                        .on_hover_text("Ajustes  (Ctrl+,)");
                    if gear.clicked() {
                        cmds.push(Cmd::OpenSettings);
                    }
                    ui.spacing_mut().item_spacing.x = 0.0;
                    if ui::window_button(ui, WindowButton::Minimize, BAR_H).clicked() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
                    }
                    let max_kind = if maximized {
                        WindowButton::Restore
                    } else {
                        WindowButton::Maximize
                    };
                    if ui::window_button(ui, max_kind, BAR_H).clicked() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!maximized));
                    }
                    if ui::window_button(ui, WindowButton::Close, BAR_H).clicked() {
                        cmds.push(Cmd::CloseWindow);
                    }
                });
            });
        w.tab_rects = tab_rects;
        w.tabbar = tabbar;

        if let Some((i, local)) = start_drag {
            if self.drag.is_none() {
                let ws = &self.windows[wi].workspaces[i];
                let color = ws
                    .color
                    .and_then(|c| tab_colors().get(c as usize).copied())
                    .unwrap_or(pal().red);
                let title = ws.title();
                let inner_min = self.windows[wi]
                    .inner
                    .map(|r| r.min.to_vec2())
                    .unwrap_or_default();
                let global = self.global_pointer(ctx, wi).unwrap_or(local + inner_min);
                self.drag = Some(TabDrag {
                    win: win_id,
                    tab: i,
                    title,
                    color,
                    start: global,
                    global,
                    active: false,
                    released: false,
                });
            }
        }
    }

    /// Fantasma de la pestaña y vista previa de dónde caería.
    fn drag_feedback(&self, ctx: &egui::Context, wi: usize) {
        let Some(d) = self.drag.as_ref().filter(|d| d.active) else {
            return;
        };
        let w = &self.windows[wi];
        let Some(inner) = w.inner else { return };
        let painter = ctx.layer_painter(egui::LayerId::new(
            egui::Order::Tooltip,
            egui::Id::new(("drag", w.id)),
        ));
        let target = self.drop_target(d);
        let local = d.global - inner.min.to_vec2();
        match target {
            DropTarget::TabBar { win, index } if win == wi => {
                let x = match (w.tab_rects.get(index), w.tab_rects.last()) {
                    (Some(r), _) => r.min.x - 3.0,
                    (None, Some(r)) => r.max.x + 3.0,
                    _ => w.tabbar.min.x + 40.0,
                };
                painter.vline(
                    x,
                    (w.tabbar.center().y - 13.0)..=(w.tabbar.center().y + 13.0),
                    Stroke::new(2.0_f32, pal().red),
                );
            }
            DropTarget::Pane {
                win, zone, rect, ..
            } if win == wi => {
                let r = zone.preview(rect).shrink(4.0);
                painter.rect_filled(
                    r,
                    CornerRadius::same(theme::RADIUS),
                    pal().red.gamma_multiply(0.14),
                );
                painter.rect_stroke(
                    r,
                    CornerRadius::same(theme::RADIUS),
                    Stroke::new(1.5_f32, pal().red.gamma_multiply(0.8)),
                    StrokeKind::Inside,
                );
            }
            DropTarget::NewWindow if w.id == d.win => {
                // El puntero salió de todas las ventanas: aviso en la de origen.
                let msg = "Suelta para abrir en una ventana nueva";
                let c = pos2(ctx.screen_rect().center().x, BAR_H + 34.0);
                let galley =
                    painter.layout_no_wrap(msg.into(), FontId::proportional(13.0), pal().text);
                let r = Rect::from_center_size(c, galley.size() + vec2(28.0, 16.0));
                painter.rect_filled(r, CornerRadius::same(8), pal().surface);
                painter.rect_stroke(
                    r,
                    CornerRadius::same(8),
                    Stroke::new(1.0_f32, pal().red),
                    StrokeKind::Inside,
                );
                painter.galley(r.center() - galley.size() / 2.0, galley, pal().text);
            }
            _ => {}
        }
        if inner.contains(d.global) {
            ui::tab_ghost(&painter, local, &d.title, d.color);
        }
    }

    /// "¿Cerrar igual?" cuando hay comandos corriendo.
    /// Puertos en uso, con botón para cerrar el proceso (pide confirmar).
    fn ports_ui(&mut self, ctx: &egui::Context, wi: usize) {
        let Some((pw, list, at)) = self.ports.as_mut() else { return };
        if *pw != wi {
            return;
        }
        // Se refresca solo cada 2 s mientras está abierto.
        if at.elapsed() > Duration::from_secs(2) {
            *list = crate::ports::list();
            *at = Instant::now();
        }
        ctx.request_repaint_after(Duration::from_secs(2));
        let p = pal();
        let id = egui::Id::new(("ports", wi));
        let mut confirm: Option<u32> = ctx.data(|d| d.get_temp(id.with("confirm")));
        let mut filter: String = ctx.data(|d| d.get_temp(id.with("filter"))).unwrap_or_default();
        let mut close = ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape));
        let mut kill = None;
        let modal = egui::Modal::new(id)
            .frame(
                Frame::new()
                    .fill(p.surface)
                    .stroke(Stroke::new(1.0_f32, p.border))
                    .corner_radius(CornerRadius::same(12))
                    .inner_margin(Margin::same(18)),
            )
            .show(ctx, |ui| {
                ui.set_width(560.0);
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Puertos en uso").size(16.0).strong());
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui.button("Cerrar").clicked() {
                            close = true;
                        }
                    });
                });
                ui.add_space(6.0);
                let edit = ui.add(
                    egui::TextEdit::singleline(&mut filter)
                        .hint_text("Filtrar por puerto o programa (3000, node…)")
                        .desired_width(f32::INFINITY),
                );
                edit.request_focus();
                ui.add_space(8.0);
                let q = filter.trim().to_lowercase();
                let shown: Vec<&crate::ports::PortInfo> = list
                    .iter()
                    .filter(|x| {
                        q.is_empty()
                            || x.port.to_string().contains(&q)
                            || x.name.to_lowercase().contains(&q)
                    })
                    .collect();
                if shown.is_empty() {
                    ui.label(RichText::new("Nada escuchando con ese filtro").color(p.muted));
                }
                egui::ScrollArea::vertical().max_height(380.0).show(ui, |ui| {
                    for x in shown {
                        ui.horizontal(|ui| {
                            ui.set_min_height(26.0);
                            let dev = x.port >= 1024;
                            ui.add_sized(
                                [64.0, 20.0],
                                egui::Label::new(
                                    RichText::new(format!(":{}", x.port))
                                        .monospace()
                                        .size(14.0)
                                        .color(if dev { p.text } else { p.muted }),
                                ),
                            );
                            ui.label(RichText::new(&x.name).color(p.text));
                            ui.label(
                                RichText::new(format!("pid {} · {}", x.pid, x.addr))
                                    .color(p.muted)
                                    .size(11.5),
                            );
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                if x.pid == 0 {
                                    return;
                                }
                                if confirm == Some(x.pid) {
                                    let b = egui::Button::new(
                                        RichText::new("¿Seguro? Cerrar").color(egui::Color32::WHITE),
                                    )
                                    .fill(p.red);
                                    if ui.add(b).clicked() {
                                        kill = Some((x.pid, x.name.clone(), x.port));
                                    }
                                } else if ui.button("Cerrar proceso").clicked() {
                                    confirm = Some(x.pid);
                                }
                            });
                        });
                    }
                });
                ui.add_space(6.0);
                ui.label(
                    RichText::new("Cerrar un proceso lo termina de inmediato (como el Administrador de tareas).")
                        .color(p.muted)
                        .size(11.0),
                );
            });
        if modal.should_close() {
            close = true;
        }
        ctx.data_mut(|d| {
            match confirm {
                Some(pid) => d.insert_temp(id.with("confirm"), pid),
                None => d.remove::<u32>(id.with("confirm")),
            }
            d.insert_temp(id.with("filter"), filter);
        });
        if let Some((pid, name, port)) = kill {
            let msg = match crate::ports::kill(pid) {
                Ok(()) => format!("Cerrado {name} (puerto {port})"),
                Err(e) => e,
            };
            self.windows[wi].toast(msg);
            ctx.data_mut(|d| d.remove::<u32>(id.with("confirm")));
            if let Some((_, list, at)) = self.ports.as_mut() {
                *list = crate::ports::list();
                *at = Instant::now();
            }
        }
        if close {
            self.ports = None;
        }
    }

    /// Diálogo para ponerle nombre a un espacio de trabajo.
    fn naming_ui(&mut self, ctx: &egui::Context, wi: usize, cmds: &mut Vec<Cmd>) {
        let w = &mut self.windows[wi];
        let Some(name) = w.naming_layout.as_mut() else { return };
        let p = pal();
        let tabs = w.workspaces.len();
        let (enter, esc) = ctx.input_mut(|i| {
            (
                i.consume_key(Modifiers::NONE, Key::Enter),
                i.consume_key(Modifiers::NONE, Key::Escape),
            )
        });
        let mut done: Option<bool> = if esc { Some(false) } else { None };
        let exists = crate::session::load_layouts()
            .iter()
            .any(|l| l.name.eq_ignore_ascii_case(name.trim()));
        let modal = egui::Modal::new(egui::Id::new(("naming", w.id)))
            .frame(
                Frame::new()
                    .fill(p.surface)
                    .stroke(Stroke::new(1.0_f32, p.border))
                    .corner_radius(CornerRadius::same(12))
                    .inner_margin(Margin::same(20)),
            )
            .show(ctx, |ui| {
                ui.set_width(400.0);
                ui.label(RichText::new("Guardar espacio de trabajo").size(16.0).strong());
                ui.add_space(4.0);
                ui.label(
                    RichText::new(format!(
                        "Guarda {} de esta ventana con sus paneles, carpetas y lo que esté corriendo (por ejemplo claude o npm run dev). Se abre desde la paleta: «Abrir espacio».",
                        if tabs == 1 { "la pestaña".to_string() } else { format!("las {tabs} pestañas") }
                    ))
                    .color(p.muted),
                );
                ui.add_space(10.0);
                let edit = ui.add(
                    egui::TextEdit::singleline(name)
                        .hint_text("Nombre, por ejemplo «API + front»")
                        .desired_width(f32::INFINITY),
                );
                edit.request_focus();
                if exists {
                    ui.label(RichText::new("Ya existe: se va a reemplazar.").color(p.yellow).size(12.0));
                }
                ui.add_space(12.0);
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let save = egui::Button::new(RichText::new("Guardar").color(egui::Color32::WHITE))
                        .fill(p.red);
                    if ui.add_enabled(!name.trim().is_empty(), save).clicked() {
                        done = Some(true);
                    }
                    if ui.button("Cancelar").clicked() {
                        done = Some(false);
                    }
                });
            });
        if enter && !name.trim().is_empty() {
            done = Some(true);
        }
        if modal.should_close() && done.is_none() {
            done = Some(false);
        }
        if let Some(yes) = done {
            if yes {
                cmds.push(Cmd::SaveLayout(name.trim().to_string()));
            }
            w.naming_layout = None;
        }
    }

    fn confirm_ui(&mut self, ctx: &egui::Context, wi: usize, cmds: &mut Vec<Cmd>) {
        let w = &mut self.windows[wi];
        let Some(c) = &w.confirm else { return };
        let p = pal();
        let (enter, esc) = ctx.input_mut(|i| {
            (
                i.consume_key(Modifiers::NONE, Key::Enter),
                i.consume_key(Modifiers::NONE, Key::Escape),
            )
        });
        let mut answer: Option<bool> = if enter {
            Some(true)
        } else if esc {
            Some(false)
        } else {
            None
        };
        let modal = egui::Modal::new(egui::Id::new(("confirm", w.id)))
            .frame(
                Frame::new()
                    .fill(p.surface)
                    .stroke(Stroke::new(1.0_f32, p.border))
                    .corner_radius(CornerRadius::same(12))
                    .inner_margin(Margin::same(20)),
            )
            .show(ctx, |ui| {
                ui.set_width(380.0);
                ui.label(RichText::new(&c.title).size(16.0).strong());
                ui.add_space(4.0);
                ui.label(RichText::new(&c.detail).color(p.muted));
                ui.label(RichText::new("Si cierras, ese proceso se detiene.").color(p.muted));
                ui.add_space(12.0);
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let close = egui::Button::new(
                        RichText::new("Cerrar igual").color(egui::Color32::WHITE),
                    )
                    .fill(p.red);
                    if ui.add(close).clicked() {
                        answer = Some(true);
                    }
                    if ui.button("Cancelar").clicked() {
                        answer = Some(false);
                    }
                });
            });
        if modal.should_close() && answer.is_none() {
            answer = Some(false);
        }
        if let Some(yes) = answer {
            if yes {
                cmds.push(c.yes.clone());
            }
            w.confirm = None;
        }
    }

    fn toast_ui(&mut self, ctx: &egui::Context, wi: usize) {
        let w = &mut self.windows[wi];
        let Some((msg, at)) = &w.toast else { return };
        let age = at.elapsed().as_secs_f32();
        if age > 2.6 {
            w.toast = None;
            return;
        }
        let alpha = (1.0 - ((age - 2.2) / 0.4).clamp(0.0, 1.0)).min((age / 0.15).min(1.0));
        let painter = ctx.layer_painter(egui::LayerId::new(
            egui::Order::Tooltip,
            egui::Id::new(("toast", w.id)),
        ));
        let galley = painter.layout_no_wrap(
            msg.clone(),
            FontId::proportional(13.0),
            pal().text.gamma_multiply(alpha),
        );
        let sr = ctx.screen_rect();
        let r = Rect::from_center_size(
            pos2(sr.center().x, sr.max.y - 44.0),
            galley.size() + vec2(28.0, 16.0),
        );
        painter.rect_filled(
            r,
            CornerRadius::same(8),
            pal().surface.gamma_multiply(alpha),
        );
        painter.rect_stroke(
            r,
            CornerRadius::same(8),
            Stroke::new(1.0_f32, pal().border.gamma_multiply(alpha)),
            StrokeKind::Inside,
        );
        painter.galley(r.center() - galley.size() / 2.0, galley, pal().text);
        ctx.request_repaint();
    }

    /// Paleta de comandos (Ctrl+Shift+P).
    fn palette_ui(&mut self, ctx: &egui::Context, wi: usize) -> Vec<Cmd> {
        let mut out = vec![];
        let active_tab = self.windows[wi].active;
        let titles: Vec<String> = self.windows[wi]
            .workspaces
            .iter()
            .map(|w| w.title())
            .collect();
        let items: Vec<PaletteItem> = palette_items(&self.profiles, active_tab, &titles);
        let w = &mut self.windows[wi];
        let Some(ps) = w.palette.as_mut() else {
            ctx.data_mut(|d| d.remove::<Instant>(egui::Id::new(("palette_in", w.id))));
            return out;
        };

        let mut matches: Vec<(f64, usize)> = items
            .iter()
            .enumerate()
            .filter_map(|(i, it)| crate::history::match_score(&ps.query, &it.label).map(|s| (s, i)))
            .collect();
        if !ps.query.trim().is_empty() {
            matches.sort_by(|a, b| b.0.total_cmp(&a.0));
        }
        let n = matches.len();
        let (up, down, enter, esc) = ctx.input_mut(|i| {
            (
                i.consume_key(Modifiers::NONE, Key::ArrowUp),
                i.consume_key(Modifiers::NONE, Key::ArrowDown),
                i.consume_key(Modifiers::NONE, Key::Enter),
                i.consume_key(Modifiers::NONE, Key::Escape),
            )
        });
        if n > 0 {
            if down {
                ps.selected = (ps.selected + 1) % n;
            }
            if up {
                ps.selected = (ps.selected + n - 1) % n;
            }
            ps.selected = ps.selected.min(n - 1);
        }
        let mut chosen = None;
        if enter && n > 0 {
            chosen = Some(matches[ps.selected].1);
        }
        let mut close = esc;
        let slide = {
            let id = egui::Id::new(("palette_in", w.id));
            let t0: Instant = ctx.data_mut(|d| d.get_temp(id)).unwrap_or_else(|| {
                let n = Instant::now();
                ctx.data_mut(|d| d.insert_temp(id, n));
                n
            });
            let t = (t0.elapsed().as_secs_f32() / 0.16).min(1.0);
            if t < 1.0 {
                ctx.request_repaint();
            }
            if egui_term::ANIMATIONS.load(std::sync::atomic::Ordering::Relaxed) {
                -14.0 * (1.0 - t).powi(3)
            } else {
                0.0
            }
        };
        egui::Area::new(egui::Id::new(("palette", w.id)))
            .order(egui::Order::Foreground)
            .anchor(Align2::CENTER_TOP, vec2(0.0, 70.0 + slide))
            .show(ctx, |ui| {
                popup_frame().inner_margin(Margin::same(14)).show(ui, |ui| {
                    ui.set_width(560.0);
                    ui.horizontal(|ui| {
                        ui::dot(ui, pal().red);
                        let before = ps.query.clone();
                        let edit = ui.add(
                            egui::TextEdit::singleline(&mut ps.query)
                                .hint_text("Escribe una acción…")
                                .font(FontId::proportional(16.0))
                                .frame(false)
                                .desired_width(f32::INFINITY),
                        );
                        edit.request_focus();
                        if ps.query != before {
                            ps.selected = 0;
                        }
                    });
                    ui.add_space(6.0);
                    let r = ui.cursor();
                    ui.painter()
                        .hline(r.x_range(), r.min.y, Stroke::new(1.0_f32, pal().border));
                    ui.add_space(6.0);
                    if n == 0 {
                        ui.label(RichText::new("Nada que coincida").color(pal().muted));
                    }
                    // Hasta 12 filas, desplazando para que la seleccionada se vea.
                    let first = ps.selected.saturating_sub(11);
                    for (row, (_, idx)) in matches.iter().enumerate().skip(first).take(12) {
                        let it = &items[*idx];
                        let (rect, resp) = ui
                            .allocate_exact_size(vec2(ui.available_width(), 28.0), Sense::click());
                        if row == ps.selected {
                            ui.painter().rect_filled(
                                rect,
                                CornerRadius::same(6),
                                theme::mix(pal().surface, pal().red, 0.16),
                            );
                        } else if resp.hovered() {
                            ui.painter()
                                .rect_filled(rect, CornerRadius::same(6), pal().hover);
                        }
                        ui.painter().text(
                            pos2(rect.min.x + 12.0, rect.center().y),
                            Align2::LEFT_CENTER,
                            &it.label,
                            FontId::proportional(13.5),
                            pal().text,
                        );
                        if !it.keys.is_empty() {
                            ui.painter().text(
                                pos2(rect.max.x - 10.0, rect.center().y),
                                Align2::RIGHT_CENTER,
                                &it.keys,
                                FontId::monospace(11.5),
                                pal().muted,
                            );
                        }
                        if resp.clicked() {
                            chosen = Some(*idx);
                        }
                    }
                });
            });
        if let Some(i) = chosen {
            out.push(items[i].cmd.clone());
            close = true;
        }
        if close {
            w.palette = None;
        }
        out
    }

    /// `.neon.toml`: nombre/color automáticos de cada pestaña y el proyecto
    /// del panel enfocado (para la paleta). Con caché: no lee disco cada frame.
    fn update_projects(&mut self) {
        let focused_win = self.windows.iter().position(|w| w.focused).unwrap_or(0);
        let mut current = None;
        for (wi, w) in self.windows.iter_mut().enumerate() {
            let active = w.active;
            for (ti, ws) in w.workspaces.iter_mut().enumerate() {
                let Some(cwd) = ws.focused_pane().map(|p| p.cwd.clone()) else { continue };
                let proj = self.projects.get(&cwd);
                ws.auto_name = proj.as_ref().and_then(|p| p.nombre.clone());
                ws.auto_color = proj.as_ref().and_then(|p| p.color_index());
                if wi == focused_win && ti == active {
                    current = proj;
                }
            }
        }
        crate::project::set_current(current);
    }

    /// Respuestas de Claude que llegaron.
    fn poll_ai(&mut self, ctx: &egui::Context) {
        use crate::ai::AiKind;
        let mut done = vec![];
        self.ai_jobs.retain(|j| match j.rx.try_recv() {
            Ok(r) => {
                done.push((j.pane_id, j.kind.clone(), r));
                false
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => true,
            Err(_) => false,
        });
        if !self.ai_jobs.is_empty() {
            ctx.request_repaint_after(Duration::from_millis(100));
        }
        for (pid, kind, r) in done {
            let Some((wi, _)) = self.locate(pid) else { continue };
            match kind {
                AiKind::Command { .. } => {
                    if let Some(p) = self.pane_mut(pid) {
                        p.ai_busy = None;
                    }
                    match r.map(|s| crate::ai::clean_command(&s)) {
                        Ok(cmd) if cmd.starts_with('#') || cmd.is_empty() => {
                            let msg = cmd.trim_start_matches('#').trim();
                            self.windows[wi].toast(if msg.is_empty() {
                                "Claude no encontró un comando para eso".to_string()
                            } else {
                                format!("Claude: {msg}")
                            });
                        }
                        Ok(cmd) => {
                            // Queda escrito para revisarlo; Enter lo ejecuta.
                            if let Some(p) = self.pane_mut(pid) {
                                p.replace_line(&cmd);
                            }
                        }
                        Err(e) => self.windows[wi].toast(e),
                    }
                }
                AiKind::Explain { .. } => {
                    if let Some(note) = self.pane_mut(pid).and_then(|p| p.ai_note.as_mut()) {
                        match r {
                            Ok(text) => note.body = Some(text),
                            Err(e) => {
                                note.body = Some(e);
                                note.error = true;
                            }
                        }
                    }
                }
            }
        }
    }

    /// Le pide a Claude que explique el bloque `idx` del panel.
    fn explain_block(&mut self, ctx: &egui::Context, pid: u64, idx: usize) {
        let Some(p) = self.pane_mut(pid) else { return };
        let Some(b) = p.blocks.get(idx) else { return };
        let command = b.command.clone();
        let output = crate::ai::tail(&p.block_output(idx), 6000);
        let (cwd, profile) = (p.cwd.clone(), p.profile);
        p.ai_note = Some(crate::pane::AiNote {
            command: command.clone(),
            output: output.clone(),
            body: None,
            error: false,
            started: Instant::now(),
        });
        let prompt = format!(
            "Carpeta: {cwd}\nComando: {command}\n\nSalida:\n{output}"
        );
        self.ai_jobs.retain(|j| !(j.pane_id == pid && matches!(j.kind, crate::ai::AiKind::Explain { .. })));
        self.ai_jobs.push(crate::ai::spawn(
            pid,
            crate::ai::AiKind::Explain { command },
            crate::ai::explain_system(shell_label(profile), os_label()),
            prompt,
            cwd,
            self.settings.ai_model.clone(),
            ctx.clone(),
        ));
    }

    /// Modo Quake: registra el atajo global y muestra u oculta la ventana.
    fn global_hotkey(&mut self, ctx: &egui::Context) {
        use crate::hotkey::{GlobalHotkey, HotkeyMsg};
        let want = self.settings.quake.then(|| self.settings.quake_key.trim().to_string());
        if self.hotkey.as_ref().map(|h| h.combo.clone()) != want {
            self.hotkey = want
                .filter(|k| crate::hotkey::parse(k).is_some())
                .map(|k| GlobalHotkey::start(&k, ctx.clone()));
        }
        let Some(h) = &self.hotkey else { return };
        // Mostrar/ocultar lo hace el hilo del atajo directamente (funciona aunque
        // la ventana esté minimizada y no se esté dibujando); aquí solo avisos.
        let mut error = None;
        while let Ok(m) = h.rx.try_recv() {
            if let HotkeyMsg::Failed(e) = m {
                error = Some(e);
            }
        }
        if let Some(e) = error {
            self.windows[0].toast(e);
        }
    }

    /// Sugerencia del historial para lo que se está escribiendo.
    fn update_ghost(&mut self, wi: usize) {
        let enabled = self.settings.autosuggest;
        let history = self.history.as_ref();
        let w = &mut self.windows[wi];
        let blocked = w.popup.is_some() || w.dirs.is_some() || w.palette.is_some();
        let Some(p) = w.ws_mut().and_then(|ws| ws.focused_pane_mut()) else {
            return;
        };
        let ok = enabled
            && !blocked
            && p.at_prompt
            && p.typed_exact
            && !p.typed.is_empty()
            && !p.is_alt_screen()
            && p.profile.has_integration();
        if !ok {
            p.ghost = None;
            return;
        }
        if p.ghost.as_ref().is_some_and(|(t, _)| *t == p.typed) {
            return;
        }
        let rest = history.and_then(|h| h.suggest(&p.typed, &p.cwd));
        p.ghost = rest.map(|r| (p.typed.clone(), r));
    }

    /// Lista de carpetas al escribir `cd ` en el prompt.
    fn dirs_ui(&mut self, ctx: &egui::Context, wi: usize) -> Vec<Cmd> {
        use crate::dirs::{DirAction, DirPopup};
        let mut actions = vec![];
        let enabled = self.settings.cd_suggest;
        let w = &mut self.windows[wi];
        // ¿Abrir?
        if w.dirs.is_none() {
            let Some(p) = w.ws().and_then(|ws| ws.focused_pane()) else {
                return actions;
            };
            let wants = p.at_prompt
                && p.typed_exact
                && p.typed.eq_ignore_ascii_case("cd ")
                && !p.is_alt_screen()
                && !p.search.open
                && p.quick.is_none();
            if !wants {
                w.dirs_dismissed = false;
                w.dirs_armed = false;
                return actions;
            }
            if !enabled || w.dirs_dismissed || w.popup.is_some() || w.palette.is_some() {
                return actions;
            }
            if !w.dirs_armed {
                w.dirs_armed = true;
                ctx.request_repaint();
                return actions;
            }
            let sep = if cfg!(windows)
                && matches!(p.profile, ProfileKind::PowerShell | ProfileKind::Cmd)
            {
                '\\'
            } else {
                '/'
            };
            w.dirs = DirPopup::new(p.id, &p.cwd, sep);
            w.dirs_armed = false;
            if w.dirs.is_none() {
                w.dirs_dismissed = true;
                return actions;
            }
        }
        let pane_id = w.dirs.as_ref().unwrap().pane_id;
        let Some((cursor, term_rect, focused)) = w
            .ws()
            .and_then(|ws| ws.pane(pane_id).map(|p| (p.cursor_row(), p.term_rect, ws.focused == pane_id)))
        else {
            w.dirs = None;
            return actions;
        };
        if !focused || w.popup.is_some() || w.palette.is_some() {
            w.dirs = None;
            return actions;
        }
        let pop = w.dirs.as_mut().unwrap();

        // ── Teclado: todo lo que se escribe filtra la lista ──
        let events: Vec<egui::Event> = ctx.input_mut(|i| {
            let (kb, rest): (Vec<_>, Vec<_>) = std::mem::take(&mut i.events)
                .into_iter()
                .partition(|e| matches!(e, egui::Event::Text(_) | egui::Event::Key { .. }));
            i.events = rest;
            kb
        });
        let mut act = DirAction::None;
        for e in events {
            match e {
                egui::Event::Text(s) => pop.type_text(&s),
                egui::Event::Key { key, pressed: true, modifiers, .. } if modifiers.is_none() || modifiers.shift_only() => match key {
                    Key::ArrowDown => {
                        pop.selected = (pop.selected + 1).min(pop.entries.len().saturating_sub(1));
                    }
                    Key::ArrowUp => pop.selected = pop.selected.saturating_sub(1),
                    Key::Tab => act = pop.descend(),
                    Key::Enter => act = pop.choose(),
                    Key::Escape => act = DirAction::Close,
                    Key::Backspace => {
                        act = match pop.back() {
                            // Sin nada más que borrar: el borrado va a la shell.
                            DirAction::Close => {
                                actions.push(Cmd::Raw(pane_id, "\x7f".into()));
                                DirAction::Close
                            }
                            a => a,
                        }
                    }
                    _ => {}
                },
                _ => {}
            }
        }

        // ── Dibujo ──
        let (Some((cursor_y, cell_h)), Some(area)) = (cursor, term_rect) else {
            w.dirs = None;
            return actions;
        };
        let row_h = 24.0;
        let max_rows = 10;
        let n = pop.entries.len();
        let first = pop.selected.saturating_sub(max_rows - 1).min(n.saturating_sub(max_rows));
        let shown = n.min(max_rows).max(1);
        let h = 30.0 + shown as f32 * row_h + 14.0;
        let width = area.width().min(460.0);
        let below = cursor_y + cell_h + 4.0 + h <= area.max.y;
        let y = if below { cursor_y + cell_h + 4.0 } else { (cursor_y - h - 4.0).max(area.min.y) };
        egui::Area::new(egui::Id::new(("dirs", w.id)))
            .order(egui::Order::Foreground)
            .fixed_pos(pos2(area.min.x, y))
            .show(ctx, |ui| {
                popup_frame().show(ui, |ui| {
                    ui.set_width(width - 16.0);
                    ui.horizontal(|ui| {
                        ui::dot(ui, pal().yellow);
                        let title = if pop.path.is_empty() && pop.query.is_empty() {
                            format!("cd {}", pop.here())
                        } else {
                            format!("cd {}{}", pop.path, pop.query)
                        };
                        ui.label(RichText::new(title).monospace().size(12.5).color(pal().text));
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            ui.label(
                                RichText::new("Tab entrar · Enter ir · Esc cerrar")
                                    .color(pal().muted)
                                    .size(11.0),
                            );
                        });
                    });
                    ui.add_space(4.0);
                    if n == 0 {
                        ui.label(RichText::new("Sin carpetas").color(pal().muted).size(12.5));
                    }
                    for i in first..(first + max_rows).min(n) {
                        let (rect, resp) =
                            ui.allocate_exact_size(vec2(ui.available_width(), row_h), Sense::click());
                        let p = ui.painter();
                        if i == pop.selected {
                            p.rect_filled(rect, CornerRadius::same(6), theme::mix(pal().surface, pal().yellow, 0.14));
                        } else if resp.hovered() {
                            p.rect_filled(rect, CornerRadius::same(6), pal().hover);
                        }
                        p.text(
                            pos2(rect.min.x + 12.0, rect.center().y),
                            Align2::LEFT_CENTER,
                            "▸",
                            FontId::proportional(11.0),
                            pal().yellow,
                        );
                        let name = &pop.entries[i];
                        p.text(
                            pos2(rect.min.x + 28.0, rect.center().y),
                            Align2::LEFT_CENTER,
                            format!("{name}/"),
                            FontId::monospace(13.0),
                            if i == pop.selected { pal().text } else { pal().muted },
                        );
                        if resp.clicked() {
                            pop.selected = i;
                            act = pop.choose();
                        }
                    }
                });
            });

        match act {
            DirAction::Write(line) => actions.push(Cmd::Insert(pane_id, line)),
            DirAction::Run(line) => {
                actions.push(Cmd::Run(pane_id, line));
                w.dirs = None;
            }
            DirAction::Close => {
                w.dirs = None;
                w.dirs_dismissed = true;
            }
            DirAction::None => {}
        }
        actions
    }

    /// Historial: lista pegada al cursor (↑) o buscador centrado (Ctrl+R).
    fn history_ui(&mut self, ctx: &egui::Context, wi: usize) -> Vec<Cmd> {
        let mut actions = vec![];
        let w = &mut self.windows[wi];
        let Some(pane_id) = w.popup.as_ref().map(|p| p.pane_id) else {
            return actions;
        };
        let Some((cwd, repo, cursor, term_rect)) =
            w.ws().and_then(|ws| ws.pane(pane_id)).map(|p| {
                (
                    p.cwd.clone(),
                    p.git.status.as_ref().map(|g| g.root.clone()),
                    p.cursor_row(),
                    p.term_rect,
                )
            })
        else {
            w.popup = None;
            return actions;
        };
        let t = now();
        let wid = w.id;
        let pop = w.popup.as_mut().unwrap();
        let inline = pop.mode == HistoryMode::Inline;

        // ── Teclado ──
        let mut close = false;
        let mut chosen: Option<(String, bool)> = None;
        let (older, newer, enter, tab, esc) = ctx.input_mut(|i| {
            (
                i.consume_key(Modifiers::NONE, Key::ArrowUp),
                i.consume_key(Modifiers::NONE, Key::ArrowDown),
                i.consume_key(Modifiers::NONE, Key::Enter)
                    | i.consume_key(Modifiers::CTRL, Key::Enter),
                i.consume_key(Modifiers::NONE, Key::Tab),
                i.consume_key(Modifiers::NONE, Key::Escape),
            )
        });
        if inline {
            // Sin cuadro de texto: lo que escribas filtra directamente.
            let typed: Vec<egui::Event> = ctx.input_mut(|i| {
                let (kb, rest): (Vec<_>, Vec<_>) = std::mem::take(&mut i.events)
                    .into_iter()
                    .partition(|e| matches!(e, egui::Event::Text(_) | egui::Event::Key { .. }));
                i.events = rest;
                kb
            });
            for e in typed {
                match e {
                    egui::Event::Text(s) => pop.query.push_str(&s),
                    egui::Event::Key {
                        key: Key::Backspace,
                        pressed: true,
                        ..
                    } => {
                        pop.query.pop();
                    }
                    _ => {}
                }
            }
        }

        if pop.computed_for.as_deref() != Some(pop.query.as_str()) {
            pop.results = match (&self.history, pop.mode) {
                (Some(h), HistoryMode::Inline) => h.recent(&pop.query, &cwd, 10),
                (Some(h), HistoryMode::Search) => h
                    .search(&pop.query, &cwd, repo.as_deref(), t, 12)
                    .into_iter()
                    .map(|s| s.entry)
                    .collect(),
                _ => vec![],
            };
            pop.selected = 0;
            pop.computed_for = Some(pop.query.clone());
        }
        let n = pop.results.len();
        // En modo ↑ la lista se dibuja al revés (lo más reciente abajo, junto al
        // prompt), así que ↑ va hacia comandos más antiguos.
        let (up_step, down_step) = if inline {
            (older, newer)
        } else {
            (newer, older)
        };
        if n > 0 && up_step {
            pop.selected = (pop.selected + 1).min(n - 1);
        }
        if down_step {
            if pop.selected == 0 {
                close = inline;
            } else {
                pop.selected -= 1;
            }
        }
        if (enter || tab) && n > 0 {
            chosen = Some((pop.results[pop.selected].command.clone(), enter));
        }
        if esc || (inline && n == 0 && (enter || tab)) {
            close = true;
        }

        // ── Dibujo ──
        let row_h = 26.0;
        let rows = n.max(1);
        if inline {
            let (Some((cursor_y, cell_h)), Some(area)) = (cursor, term_rect) else {
                w.popup = None;
                return actions;
            };
            let h = 30.0 + rows as f32 * row_h + 14.0;
            let width = area.width().min(620.0);
            let above = cursor_y - h - 4.0 >= area.min.y;
            let y = if above {
                cursor_y - h - 4.0
            } else {
                cursor_y + cell_h + 4.0
            };
            egui::Area::new(egui::Id::new(("history_inline", wid)))
                .order(egui::Order::Foreground)
                .fixed_pos(pos2(area.min.x, y))
                .show(ctx, |ui| {
                    popup_frame().show(ui, |ui| {
                        ui.set_width(width - 16.0);
                        ui.horizontal(|ui| {
                            ui::dot(ui, pal().red);
                            let q = if pop.query.is_empty() {
                                RichText::new("Historial").color(pal().muted)
                            } else {
                                RichText::new(&pop.query).color(pal().text).monospace()
                            };
                            ui.label(q.size(12.5));
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                ui.label(
                                    RichText::new("Enter ejecutar · Tab editar · Esc cerrar")
                                        .color(pal().muted)
                                        .size(11.0),
                                );
                            });
                        });
                        ui.add_space(4.0);
                        if n == 0 {
                            ui.label(
                                RichText::new("Nada que coincida")
                                    .color(pal().muted)
                                    .size(12.5),
                            );
                        }
                        for i in (0..n).rev() {
                            if ui::history_row(
                                ui,
                                &pop.results[i],
                                i == pop.selected,
                                t,
                                row_h,
                                true,
                            ) {
                                chosen = Some((pop.results[i].command.clone(), true));
                            }
                        }
                    });
                });
        } else {
            egui::Area::new(egui::Id::new(("history_search", wid)))
                .order(egui::Order::Foreground)
                .anchor(Align2::CENTER_TOP, vec2(0.0, 80.0))
                .show(ctx, |ui| {
                    popup_frame().inner_margin(Margin::same(14)).show(ui, |ui| {
                        ui.set_width(620.0);
                        ui.horizontal(|ui| {
                            ui::dot(ui, pal().red);
                            let edit = ui.add(
                                egui::TextEdit::singleline(&mut pop.query)
                                    .hint_text("Buscar en el historial…")
                                    .font(FontId::proportional(16.0))
                                    .frame(false)
                                    .desired_width(f32::INFINITY),
                            );
                            edit.request_focus();
                        });
                        ui.add_space(6.0);
                        let r = ui.cursor();
                        ui.painter().hline(r.x_range(), r.min.y, Stroke::new(1.0_f32, pal().border));
                        ui.add_space(6.0);
                        if n == 0 {
                            ui.label(RichText::new("Sin resultados").color(pal().muted));
                        }
                        for i in 0..n {
                            if ui::history_row(ui, &pop.results[i], i == pop.selected, t, row_h, false) {
                                chosen = Some((pop.results[i].command.clone(), true));
                            }
                        }
                        ui.add_space(6.0);
                        ui.label(
                            RichText::new("Enter: ejecutar   ·   Tab: dejar escrito para editar   ·   Esc: cerrar")
                                .color(pal().muted)
                                .size(11.5),
                        );
                    });
                });
        }

        if let Some((cmd, run)) = chosen {
            actions.push(if run {
                Cmd::Run(pane_id, cmd)
            } else {
                Cmd::Insert(pane_id, cmd)
            });
            close = true;
        }
        if close {
            w.popup = None;
        }
        actions
    }
}

enum DropTarget {
    None,
    TabBar {
        win: usize,
        index: usize,
    },
    Pane {
        win: usize,
        pane: u64,
        zone: DropZone,
        rect: Rect,
    },
    NewWindow,
}

/// Notificación de Windows (en un hilo aparte: puede tardar).
fn notify(title: &str, body: String) {
    if std::env::var_os("NEON_DEBUG").is_some() {
        eprintln!("[neon] aviso: {title} — {body}");
    }
    let title = title.to_string();
    std::thread::spawn(move || {
        let mut n = notify_rust::Notification::new();
        n.summary(&title).body(&body).appname("Neon");
        // Instalado: la notificación sale con el nombre y el ícono de Neon.
        #[cfg(windows)]
        if let Some(id) = crate::install::notification_app_id() {
            n.app_id(id);
        }
        let _ = n.show();
    });
}

impl eframe::App for App {
    /// Fondo transparente: así funcionan la opacidad y Mica/Acrylic.
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        [0.0; 4]
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if std::env::var_os("NEON_FPS").is_some() {
            use std::sync::atomic::{AtomicU64, Ordering};
            static FRAMES: AtomicU64 = AtomicU64::new(0);
            static SINCE: std::sync::Mutex<Option<Instant>> = std::sync::Mutex::new(None);
            FRAMES.fetch_add(1, Ordering::Relaxed);
            let mut since = SINCE.lock().unwrap();
            let t = since.get_or_insert_with(Instant::now);
            if t.elapsed() >= Duration::from_secs(1) {
                eprintln!(
                    "[neon] {} fps, cpu/frame {:.1} ms",
                    FRAMES.swap(0, Ordering::Relaxed),
                    _frame.info().cpu_usage.unwrap_or(0.0) * 1000.0,
                );
                *t = Instant::now();
            }
        }
        egui_term::ANIMATIONS.store(
            self.settings.animations,
            std::sync::atomic::Ordering::Relaxed,
        );
        if let Some(i) = &self.intro {
            if !i.show(ctx) {
                self.intro = None;
            }
        }
        self.global_hotkey(ctx);
        self.update_projects();
        if self.trim_at.is_some_and(|t| Instant::now() >= t) {
            self.trim_at = None;
            crate::window::release_memory();
        }
        self.poll_ai(ctx);
        // Cierres animados que ya terminaron.
        let done: Vec<u64> = self
            .closing
            .iter()
            .filter(|(_, t)| t.elapsed() >= crate::ui::CLOSE_ANIM)
            .map(|(id, _)| *id)
            .collect();
        for id in done {
            self.closing.retain(|(c, _)| *c != id);
            if let Some((wi, ti)) = self.locate(id) {
                self.close_pane_now(ctx, wi, ti, id);
            }
        }
        if !self.closing.is_empty() {
            ctx.request_repaint();
        }
        egui_term::VIRTUAL_SELECT.store(true, std::sync::atomic::Ordering::Relaxed);
        egui_term::MOUSE_SELECT.store(
            self.settings.mouse_select,
            std::sync::atomic::Ordering::Relaxed,
        );
        self.pump_events(ctx);
        self.pump_updates(ctx);

        // Ventana principal.
        self.window_ui(ctx, 0);

        // Las demás ventanas (mismo proceso, cada una con su viewport).
        let extra: Vec<(u64, ViewportId, ViewportBuilder)> = self
            .windows
            .iter()
            .skip(1)
            .filter_map(|w| Some((w.id, w.viewport, w.builder.clone()?)))
            .collect();
        for (id, vp, builder) in extra {
            ctx.show_viewport_immediate(vp, builder, |ctx, _class| {
                if let Some(wi) = self.win_index(id) {
                    self.window_ui(ctx, wi);
                }
            });
        }

        // Arrastre de pestañas: posición global y si se soltó.
        if let Some(d) = &mut self.drag {
            if let (Some((x, y)), Some(down)) = (
                crate::window::global_cursor_px(),
                crate::window::left_button_down(),
            ) {
                let ppp = ctx
                    .input(|i| i.viewport().native_pixels_per_point)
                    .unwrap_or(1.0);
                d.global = pos2(x / ppp, y / ppp);
                d.released = !down;
            }
            if d.global.distance(d.start) > 10.0 {
                d.active = true;
            }
            if d.released {
                self.finish_drag(ctx);
            }
            ctx.request_repaint();
        }

        // Comandos pedidos durante el frame.
        for (win, cmd) in std::mem::take(&mut self.pending) {
            self.exec(ctx, win, cmd);
        }

        // Estilo nativo (esquinas, sombra) para ventanas recién creadas.
        if self.style_frames > 0 {
            self.style_frames -= 1;
            if self.style_frames <= 1 {
                crate::window::style_all_windows();
            }
        }

        self.settings_housekeeping(ctx);

        if self.last_save.elapsed() > Duration::from_secs(30) {
            self.save_session();
        }

        // Copias hechas por comandos de este frame (p. ej. «Copiar salida»).
        let copied: Vec<String> = ctx.output(|o| {
            o.commands
                .iter()
                .filter_map(|c| match c {
                    egui::OutputCommand::CopyText(t) => Some(t.clone()),
                    _ => None,
                })
                .collect()
        });
        for t in copied {
            self.remember_clip(t);
        }
        // Para notar eventos de la shell y refrescos de git aunque no haya input.
        ctx.request_repaint_after(Duration::from_millis(500));
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.save_session();
    }
}

/// Nombre de la shell para las instrucciones a Claude.
fn shell_label(k: ProfileKind) -> &'static str {
    match k {
        ProfileKind::PowerShell => "PowerShell",
        ProfileKind::Cmd => "cmd.exe",
        ProfileKind::GitBash => "Git Bash (bash en Windows)",
        ProfileKind::Wsl => "bash en WSL (Linux)",
        ProfileKind::Bash => "bash",
    }
}

fn os_label() -> &'static str {
    if cfg!(windows) { "Windows" } else if cfg!(target_os = "macos") { "macOS" } else { "Linux" }
}
