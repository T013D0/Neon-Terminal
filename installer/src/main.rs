//! Instalador de Neon, al estilo del de Discord: una ventanita sin bordes con
//! el logo (un planeta con su anillo meciéndose).
//!
//! Pantallas: bienvenida con opciones → instalando → listo. Si Neon ya está
//! instalado muestra Actualizar/Reparar y Desinstalar (con confirmación).
//! Con `--auto` (lo usa la actualización desde GitHub) actualiza sin preguntar.
//!
//! Lleva neon.exe adentro (ver build.rs). Lo copia a
//! %LOCALAPPDATA%\Programs\neon y después llama a `neon --instalar --silencioso`
//! (más las opciones elegidas), que registra todo en Windows.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use egui::{
    Align2, Color32, CornerRadius, FontId, Pos2, Rect, Sense, Stroke, StrokeKind, pos2, vec2,
};
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

static PAYLOAD: &[u8] = include_bytes!(env!("NEON_PAYLOAD"));
// conpty.dll y OpenConsole.exe de Windows Terminal (si se incluyeron al compilar).
include!(concat!(env!("OUT_DIR"), "/extra.rs"));
const VERSION: &str = env!("CARGO_PKG_VERSION");

const BG_TOP: Color32 = Color32::from_rgb(0x1d, 0x1e, 0x27);
const BG_BOTTOM: Color32 = Color32::from_rgb(0x0b, 0x0c, 0x10);
const RED: Color32 = Color32::from_rgb(0xed, 0x4a, 0x5b);
const TEXT: Color32 = Color32::from_rgb(0xe7, 0xe9, 0xf0);
const MUTED: Color32 = Color32::from_rgb(0x6e, 0x73, 0x85);
const TRACK: Color32 = Color32::from_rgb(0x24, 0x26, 0x2f);
const GREEN: Color32 = Color32::from_rgb(0x7e, 0xe7, 0x87);

const W: f32 = 380.0;
const H: f32 = 480.0;

fn main() -> eframe::Result {
    let icon = eframe::icon_data::from_png_bytes(include_bytes!("../../assets/icon-256.png"))
        .unwrap_or_default();
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Instalar Neon")
            .with_inner_size([W, H])
            .with_resizable(false)
            .with_decorations(false)
            .with_icon(std::sync::Arc::new(icon)),
        centered: true,
        ..Default::default()
    };
    eframe::run_native(
        "NeonSetup",
        options,
        Box::new(|cc| Ok(Box::new(Setup::new(cc)))),
    )
}

// ───────────────────────────── Instalación ─────────────────────────────

/// Qué integrar con Windows (cada casilla de la pantalla de bienvenida).
#[derive(Clone, Copy, PartialEq)]
struct Opts {
    desktop: bool,
    start_menu: bool,
    context_menu: bool,
    explorer: bool,
    launch: bool,
    /// Actualización automática: no tocar la integración que ya había.
    keep: bool,
}

impl Default for Opts {
    fn default() -> Self {
        Self {
            desktop: true,
            start_menu: true,
            context_menu: true,
            explorer: true,
            launch: true,
            keep: false,
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Job {
    Install,
    Repair,
    Update,
    Uninstall,
}

enum Msg {
    Step(String, f32),
    Done,
    Error(String),
}

fn install_dir() -> PathBuf {
    if cfg!(windows) {
        dirs::data_local_dir()
            .unwrap_or_else(std::env::temp_dir)
            .join("Programs")
            .join("neon")
    } else {
        // En otros sistemas solo sirve para probar la interfaz.
        std::env::temp_dir().join("neon-setup-prueba")
    }
}

fn exe_path() -> PathBuf {
    install_dir().join(if cfg!(windows) { "neon.exe" } else { "neon" })
}

/// Versión de Neon que ya está instalada (None si no lo está).
fn installed_version() -> Option<String> {
    if !exe_path().is_file() {
        return None;
    }
    let v = std::fs::read_to_string(install_dir().join("version.txt")).unwrap_or_default();
    let v = v.trim();
    Some(if v.is_empty() { "?".into() } else { v.into() })
}

fn desktop_link_exists() -> bool {
    dirs::desktop_dir().is_some_and(|d| d.join("Neon.lnk").exists())
}

fn run_job(tx: mpsc::Sender<Msg>, ctx: egui::Context, job: Job, opts: Opts, purge: bool) {
    let send = |m: Msg| {
        let _ = tx.send(m);
        ctx.request_repaint();
    };
    let pause = |ms| std::thread::sleep(Duration::from_millis(ms));
    let result = (|| -> Result<(), String> {
        send(Msg::Step("Preparando…".into(), 0.12));
        pause(700);
        if job == Job::Uninstall {
            return uninstall(&send, &pause, purge);
        }

        let dir = install_dir();
        std::fs::create_dir_all(&dir)
            .map_err(|e| format!("No pude crear {}: {e}", dir.display()))?;
        let exe = exe_path();
        let updating = exe.exists();
        send(Msg::Step(
            match job {
                Job::Repair => "Reparando Neon…",
                Job::Update => "Actualizando Neon…",
                _ if updating => "Actualizando Neon…",
                _ => "Copiando Neon…",
            }
            .into(),
            0.42,
        ));
        // Si Neon está abierto, su exe está bloqueado: se puede renombrar pero
        // no sobrescribir. Lo movemos aparte y escribimos el nuevo.
        let old = dir.join("neon.old.exe");
        let _ = std::fs::remove_file(&old);
        if updating {
            let _ = std::fs::rename(&exe, &old);
        }
        std::fs::write(&exe, PAYLOAD).map_err(|e| format!("No pude copiar Neon: {e}"))?;
        let _ = std::fs::write(dir.join("version.txt"), VERSION);
        // Igual que el exe: si Neon está abierto, conpty.dll está en uso.
        for (name, bytes) in EXTRA {
            let file = dir.join(name);
            let old = dir.join(format!("{name}.old"));
            let _ = std::fs::remove_file(&old);
            if file.exists() {
                let _ = std::fs::rename(&file, &old);
            }
            let _ = std::fs::write(&file, bytes);
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&exe, std::fs::Permissions::from_mode(0o755));
        }
        pause(700);

        send(Msg::Step("Integrando con Windows…".into(), 0.75));
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            let mut cmd = std::process::Command::new(&exe);
            cmd.args(["--instalar", "--silencioso"]);
            if opts.keep {
                cmd.arg("--mantener");
            }
            if opts.desktop {
                cmd.arg("--escritorio");
            }
            if !opts.start_menu {
                cmd.arg("--sin-inicio");
            }
            if !opts.context_menu {
                cmd.arg("--sin-menu");
            }
            if !opts.explorer {
                cmd.arg("--sin-explorador");
            }
            let out = cmd
                .creation_flags(CREATE_NO_WINDOW)
                .output()
                .map_err(|e| format!("No pude ejecutar Neon: {e}"))?;
            if !out.status.success() {
                let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
                return Err(if err.is_empty() {
                    "Falló el registro en Windows.".into()
                } else {
                    err
                });
            }
        }
        #[cfg(not(windows))]
        let _ = opts;
        pause(700);
        Ok(())
    })();
    match result {
        Ok(()) => send(Msg::Done),
        Err(e) => send(Msg::Error(e)),
    }
}

/// Quita la integración con Windows y borra los archivos.
fn uninstall(send: &dyn Fn(Msg), pause: &dyn Fn(u64), purge: bool) -> Result<(), String> {
    send(Msg::Step("Quitando Neon de Windows…".into(), 0.5));
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        let exe = exe_path();
        if exe.is_file() {
            let mut cmd = std::process::Command::new(&exe);
            cmd.args(["--desinstalar", "--silencioso"]);
            if purge {
                cmd.arg("--borrar-datos");
            }
            let out = cmd
                .creation_flags(CREATE_NO_WINDOW)
                .output()
                .map_err(|e| format!("No pude ejecutar Neon: {e}"))?;
            if !out.status.success() {
                let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
                return Err(if err.is_empty() {
                    "Falló la desinstalación.".into()
                } else {
                    err
                });
            }
        }
    }
    #[cfg(not(windows))]
    {
        let _ = purge;
        let _ = std::fs::remove_dir_all(install_dir());
    }
    pause(900);
    // En Windows la carpeta se borra unos segundos después (un exe no puede borrarse mientras corre).
    send(Msg::Step("Quitando archivos…".into(), 0.85));
    pause(900);
    Ok(())
}

// ───────────────────────────── Interfaz ─────────────────────────────

#[derive(PartialEq)]
enum State {
    /// Animación de entrada del logo.
    Intro,
    /// Primera vez: elegir qué integrar y pulsar Instalar.
    Welcome,
    /// Ya está instalado: actualizar/reparar o desinstalar.
    Manage,
    /// ¿Seguro que quieres desinstalar?
    ConfirmUninstall,
    /// Instalando, reparando, actualizando o desinstalando.
    Working,
    Done {
        at: Instant,
        /// Abrir Neon solo y cerrar el instalador.
        auto: bool,
        launched: bool,
    },
    Error(String),
}

/// Lo que el usuario pidió con un botón (se aplica al final del frame).
enum Act {
    Start(Job),
    Confirm,
    Back,
    Open,
    Close,
}

struct Setup {
    planet: egui::TextureHandle,
    ring: egui::TextureHandle,
    start: Instant,
    state: State,
    job: Job,
    opts: Opts,
    /// Al desinstalar: borrar también historial y ajustes.
    purge: bool,
    installed: Option<String>,
    status: String,
    detail: String,
    target: f32,
    shown: f32,
    /// Lanzado por la actualización de Neon: si ya está instalado, actualiza sin preguntar.
    auto: bool,
    /// 0 = logo chico arriba (pantallas con opciones), 1 = logo grande (progreso).
    k: f32,
    rx: Option<Receiver<Msg>>,
}

impl Setup {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        install_fonts(&cc.egui_ctx);
        round_window(cc);
        // El logo en dos capas (planeta y anillo) para animar el anillo.
        let load = |name: &str, bytes: &[u8]| {
            let icon = eframe::icon_data::from_png_bytes(bytes).unwrap_or_default();
            let img = egui::ColorImage::from_rgba_unmultiplied(
                [icon.width as usize, icon.height as usize],
                &icon.rgba,
            );
            cc.egui_ctx
                .load_texture(name, img, egui::TextureOptions::LINEAR)
        };
        let planet = load("planet", include_bytes!("../../assets/planet.png"));
        let ring = load("ring", include_bytes!("../../assets/ring.png"));
        Self {
            planet,
            ring,
            start: Instant::now(),
            state: State::Intro,
            job: Job::Install,
            opts: Opts::default(),
            purge: false,
            installed: installed_version(),
            status: String::new(),
            detail: String::new(),
            target: 0.0,
            shown: 0.0,
            auto: std::env::args().any(|a| a == "--auto"),
            k: 1.0,
            rx: None,
        }
    }

    fn begin(&mut self, ctx: &egui::Context, job: Job) {
        let (tx, rx) = mpsc::channel();
        let ctx = ctx.clone();
        let (opts, purge) = (self.opts, self.purge);
        std::thread::spawn(move || run_job(tx, ctx, job, opts, purge));
        self.rx = Some(rx);
        self.job = job;
        self.state = State::Working;
        self.status = "Preparando…".into();
        self.detail.clear();
        self.target = 0.04;
        self.shown = 0.0;
    }

    /// Texto de la pantalla de opciones según si ya está instalado.
    fn enter_menu(&mut self) {
        self.installed = installed_version();
        if self.installed.is_some() {
            self.opts.desktop = desktop_link_exists();
            self.state = State::Manage;
        } else {
            self.state = State::Welcome;
        }
    }

    fn open_neon(&self) {
        if cfg!(windows) {
            let _ = std::process::Command::new(exe_path())
                .current_dir(dirs::home_dir().unwrap_or_default())
                .spawn();
        }
    }
}

impl eframe::App for Setup {
    fn clear_color(&self, _: &egui::Visuals) -> [f32; 4] {
        [0.0; 4]
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let t = self.start.elapsed().as_secs_f32();

        // Después de la animación de entrada, aparece la pantalla de opciones.
        if self.state == State::Intro && t > 0.9 {
            self.enter_menu();
            if self.auto && matches!(self.state, State::Manage) {
                // Actualización lanzada desde Neon: sin preguntas, y vuelve a abrir Neon.
                self.opts.keep = true;
                self.opts.launch = true;
                self.begin(ctx, Job::Update);
            }
        }
        if let Some(rx) = &self.rx {
            while let Ok(m) = rx.try_recv() {
                match m {
                    Msg::Step(s, p) => {
                        self.status = s;
                        self.target = p;
                    }
                    Msg::Done => {
                        let launch = self.opts.launch && self.job != Job::Uninstall;
                        self.status = match (self.job, launch) {
                            (Job::Install, true) => "¡Listo! Abriendo Neon…",
                            (Job::Install, false) => "¡Neon quedó instalado!",
                            (Job::Repair, true) => "Neon quedó reparado. Abriendo…",
                            (Job::Repair, false) => "Neon quedó reparado",
                            (Job::Update, true) => "Neon se actualizó. Abriendo…",
                            (Job::Update, false) => "Neon se actualizó",
                            (Job::Uninstall, _) => "Neon se desinstaló",
                        }
                        .into();
                        self.detail = if self.job == Job::Uninstall {
                            if self.purge {
                                "También se borraron tu historial y ajustes."
                            } else {
                                "Tu historial y ajustes se conservaron."
                            }
                            .into()
                        } else {
                            String::new()
                        };
                        self.target = 1.0;
                        self.state = State::Done {
                            at: Instant::now(),
                            auto: launch,
                            launched: false,
                        };
                    }
                    Msg::Error(e) => {
                        self.status = "No se pudo completar".into();
                        self.state = State::Error(e);
                    }
                }
            }
        }
        // La barra avanza suave hacia el objetivo.
        let dt = ctx.input(|i| i.stable_dt).min(0.1);
        self.shown += (self.target - self.shown) * (1.0 - (-dt * 6.0).exp());

        // El logo se achica para dejar lugar a las opciones y crece al trabajar.
        let compact = matches!(
            self.state,
            State::Welcome | State::Manage | State::ConfirmUninstall
        );
        let goal = if compact { 0.0 } else { 1.0 };
        self.k += (goal - self.k) * (1.0 - (-dt * 9.0).exp());

        // Al terminar con "abrir al terminar": abre Neon y cierra el instalador.
        let mut open = false;
        let mut close = false;
        if let State::Done {
            at,
            auto: true,
            launched,
        } = &mut self.state
        {
            if !*launched && at.elapsed() > Duration::from_millis(900) {
                *launched = true;
                open = true;
            }
            if at.elapsed() > Duration::from_millis(1500) {
                close = true;
            }
        }
        if open {
            self.open_neon();
        }
        if close {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }

        egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show(ctx, |ui| self.draw(ui, t));
        ctx.request_repaint();
    }
}

impl Setup {
    fn draw(&mut self, ui: &mut egui::Ui, t: f32) {
        let rect = ui.max_rect();
        let p = ui.painter().clone();

        // Toda la ventana se puede arrastrar.
        let bg = ui.interact(rect, ui.id().with("drag"), Sense::click_and_drag());
        if bg.drag_started() {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
        }

        // Fondo: degradado vertical (en Windows 11 las esquinas las redondea el sistema).
        let mut mesh = egui::Mesh::default();
        mesh.colored_vertex(rect.left_top(), BG_TOP);
        mesh.colored_vertex(rect.right_top(), BG_TOP);
        mesh.colored_vertex(rect.left_bottom(), BG_BOTTOM);
        mesh.colored_vertex(rect.right_bottom(), BG_BOTTOM);
        mesh.add_triangle(0, 1, 2);
        mesh.add_triangle(1, 2, 3);
        p.add(mesh);
        if !cfg!(windows) {
            p.rect_stroke(rect, 0.0, Stroke::new(1.0_f32, TRACK), StrokeKind::Inside);
        }

        // Entrada: el logo aparece y sube un poco.
        let intro = ease_out((t / 0.7).min(1.0));
        let done = matches!(self.state, State::Done { .. });
        let error = matches!(self.state, State::Error(_));
        let working = matches!(self.state, State::Working);
        // Interpolación entre el diseño compacto (opciones) y el grande (progreso).
        let k = self.k;
        let lerp = |a: f32, b: f32| a + (b - a) * k;
        let center = pos2(
            rect.center().x,
            rect.min.y + lerp(100.0, 175.0) + (1.0 - intro) * 18.0,
        );

        // Halo rojo que late detrás del logo.
        let pulse = 0.5 + 0.5 * (t * 2.2).sin();
        let halo_color = if error { MUTED } else { RED };
        let halo_strength = if done { 1.0 } else { 0.55 + 0.45 * pulse };
        glow(
            &p,
            center,
            lerp(95.0, 135.0),
            halo_color,
            0.16 * halo_strength * intro,
        );

        let scale = 1.0 + 0.02 * pulse * (!done as u8 as f32);
        let size = lerp(104.0, 150.0) * scale;
        let tint = Color32::from_white_alpha((255.0 * intro) as u8);
        let logo_rect = Rect::from_center_size(center, vec2(size, size));
        // El anillo se mece suave mientras instala; al terminar da una vuelta y se queda quieto.
        let angle = match &self.state {
            State::Done { at, .. } => {
                let kk = ease_out((at.elapsed().as_secs_f32() / 0.8).min(1.0));
                std::f32::consts::TAU * kk
            }
            _ => 0.12 * (t * 1.4).sin() * intro,
        };
        egui::Image::new(&self.planet)
            .tint(tint)
            .paint_at(ui, logo_rect);
        egui::Image::new(&self.ring)
            .tint(tint)
            .rotate(angle, vec2(0.5, 0.5))
            .paint_at(ui, logo_rect);

        // Textos.
        let fade = |c: Color32| c.gamma_multiply(intro);
        p.text(
            pos2(rect.center().x, rect.min.y + lerp(176.0, 290.0)),
            Align2::CENTER_CENTER,
            "Neon",
            FontId::proportional(lerp(26.0, 30.0)),
            fade(TEXT),
        );
        let status_color = if error {
            RED
        } else if done {
            GREEN
        } else {
            MUTED
        };
        // Subtítulo de las pantallas de opciones.
        let (status, detail): (String, String) = match &self.state {
            State::Welcome => ("Una terminal moderna, a tu medida".into(), String::new()),
            State::Manage => (
                "Neon ya está instalado".into(),
                format!(
                    "Instalada: v{} · Este instalador: v{VERSION}",
                    self.installed.as_deref().unwrap_or("?")
                ),
            ),
            State::ConfirmUninstall => (
                "¿Desinstalar Neon?".into(),
                "Se quitan los accesos directos y el menú contextual.".into(),
            ),
            _ => (self.status.clone(), self.detail.clone()),
        };
        p.text(
            pos2(rect.center().x, rect.min.y + lerp(206.0, 326.0)),
            Align2::CENTER_CENTER,
            &status,
            FontId::proportional(14.0),
            fade(status_color),
        );
        if !detail.is_empty() && !error {
            p.text(
                pos2(rect.center().x, rect.min.y + lerp(228.0, 388.0)),
                Align2::CENTER_CENTER,
                &detail,
                FontId::proportional(11.5),
                fade(MUTED.gamma_multiply(0.85)),
            );
        }

        // Barra de progreso con un brillo que la recorre.
        if matches!(
            self.state,
            State::Intro | State::Working | State::Done { .. }
        ) {
            let bar =
                Rect::from_center_size(pos2(rect.center().x, rect.min.y + 362.0), vec2(220.0, 4.0));
            p.rect_filled(bar, CornerRadius::same(2), fade(TRACK));
            let fill =
                Rect::from_min_size(bar.min, vec2(bar.width() * self.shown.clamp(0.0, 1.0), 4.0));
            let bar_color = if done { GREEN } else { RED };
            p.rect_filled(fill, CornerRadius::same(2), fade(bar_color));
            if working && fill.width() > 12.0 {
                let x = fill.min.x + ((t * 0.9).fract() * (fill.width() + 40.0)) - 20.0;
                let shine = Rect::from_min_max(
                    pos2(x.max(fill.min.x), fill.min.y),
                    pos2((x + 20.0).min(fill.max.x), fill.max.y),
                );
                if shine.width() > 0.0 {
                    p.rect_filled(shine, CornerRadius::same(2), Color32::from_white_alpha(70));
                }
            }
        }

        // Botones y casillas de cada pantalla.
        let cx = rect.center().x;
        let by = rect.max.y - 52.0;
        let mut act: Option<Act> = None;
        let half = |dx: f32| Rect::from_center_size(pos2(cx + dx, by), vec2(116.0, 34.0));
        let row = |i: usize| {
            Rect::from_min_size(
                pos2(cx - 150.0, rect.min.y + 248.0 + i as f32 * 26.0),
                vec2(300.0, 24.0),
            )
        };
        match self.state {
            State::Welcome | State::Manage => {
                let o = &mut self.opts;
                checkbox(
                    ui,
                    row(0),
                    "Acceso directo en el escritorio",
                    &mut o.desktop,
                );
                checkbox(
                    ui,
                    row(1),
                    "Acceso directo en el menú Inicio",
                    &mut o.start_menu,
                );
                checkbox(
                    ui,
                    row(2),
                    "«Abrir en Neon» al hacer click derecho",
                    &mut o.context_menu,
                );
                checkbox(
                    ui,
                    row(3),
                    "«neon» en el Explorador y Win+R",
                    &mut o.explorer,
                );
                checkbox(ui, row(4), "Abrir Neon al terminar", &mut o.launch);
                if matches!(self.state, State::Welcome) {
                    let r = Rect::from_center_size(pos2(cx, by - 4.0), vec2(220.0, 40.0));
                    if button(ui, r, "Instalar", true).clicked() {
                        act = Some(Act::Start(Job::Install));
                    }
                } else {
                    let up = self.installed.as_deref() != Some(VERSION);
                    let (label, job) = if up {
                        ("Actualizar", Job::Update)
                    } else {
                        ("Reparar", Job::Repair)
                    };
                    if button(ui, half(-64.0), label, true).clicked() {
                        act = Some(Act::Start(job));
                    }
                    if button(ui, half(64.0), "Desinstalar", false).clicked() {
                        act = Some(Act::Confirm);
                    }
                }
            }
            State::ConfirmUninstall => {
                checkbox(
                    ui,
                    row(0),
                    "Borrar también mi historial y ajustes",
                    &mut self.purge,
                );
                if button(ui, half(-64.0), "Desinstalar", true).clicked() {
                    act = Some(Act::Start(Job::Uninstall));
                }
                if button(ui, half(64.0), "Cancelar", false).clicked() {
                    act = Some(Act::Back);
                }
            }
            State::Done { auto: false, .. } => {
                if self.job == Job::Uninstall {
                    let r = Rect::from_center_size(pos2(cx, by - 4.0), vec2(160.0, 38.0));
                    if button(ui, r, "Cerrar", true).clicked() {
                        act = Some(Act::Close);
                    }
                } else {
                    if button(ui, half(-64.0), "Abrir Neon", true).clicked() {
                        act = Some(Act::Open);
                    }
                    if button(ui, half(64.0), "Cerrar", false).clicked() {
                        act = Some(Act::Close);
                    }
                }
            }
            State::Error(_) => {
                if let State::Error(e) = &self.state {
                    let area =
                        Rect::from_center_size(pos2(cx, rect.min.y + 378.0), vec2(320.0, 40.0));
                    let msg = e.clone();
                    ui.scope_builder(egui::UiBuilder::new().max_rect(area), |ui| {
                        ui.vertical_centered(|ui| {
                            ui.label(egui::RichText::new(msg).size(12.0).color(MUTED));
                        });
                    });
                }
                if button(ui, half(-64.0), "Reintentar", true).clicked() {
                    act = Some(Act::Start(self.job));
                }
                if button(ui, half(64.0), "Volver", false).clicked() {
                    act = Some(Act::Back);
                }
            }
            _ => {}
        }

        // Pie: versión.
        p.text(
            pos2(cx, rect.max.y - 18.0),
            Align2::CENTER_CENTER,
            format!("v{VERSION}"),
            FontId::proportional(11.0),
            fade(MUTED.gamma_multiply(0.7)),
        );

        // Cerrar (no mientras está trabajando).
        if !working {
            let r = Rect::from_center_size(
                pos2(rect.max.x - 22.0, rect.min.y + 22.0),
                vec2(28.0, 28.0),
            );
            let resp = ui.interact(r, ui.id().with("close"), Sense::click());
            let c = if resp.hovered() { TEXT } else { MUTED };
            if resp.hovered() {
                p.rect_filled(r, CornerRadius::same(6), TRACK);
            }
            let s = Stroke::new(1.3_f32, c);
            let m = r.center();
            p.line_segment([m + vec2(-5.0, -5.0), m + vec2(5.0, 5.0)], s);
            p.line_segment([m + vec2(-5.0, 5.0), m + vec2(5.0, -5.0)], s);
            if resp.clicked() {
                act = Some(Act::Close);
            }
        }

        match act {
            Some(Act::Start(job)) => self.begin(ui.ctx(), job),
            Some(Act::Confirm) => {
                self.purge = false;
                self.state = State::ConfirmUninstall;
            }
            Some(Act::Back) => self.enter_menu(),
            Some(Act::Open) => {
                self.open_neon();
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
            }
            Some(Act::Close) => ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close),
            None => {}
        }
    }
}

/// Casilla con texto: toda la fila es clickeable.
fn checkbox(ui: &mut egui::Ui, r: Rect, label: &str, on: &mut bool) {
    let resp = ui.interact(r, ui.id().with(label), Sense::click());
    if resp.clicked() {
        *on = !*on;
    }
    let p = ui.painter();
    if resp.hovered() {
        p.rect_filled(r.expand2(vec2(6.0, 1.0)), CornerRadius::same(6), TRACK);
    }
    let b = Rect::from_center_size(pos2(r.min.x + 11.0, r.center().y), vec2(18.0, 18.0));
    if *on {
        p.rect_filled(b, CornerRadius::same(5), RED);
        let c = b.center();
        p.line_segment(
            [c + vec2(-4.5, 0.5), c + vec2(-1.5, 3.5)],
            Stroke::new(2.0_f32, Color32::WHITE),
        );
        p.line_segment(
            [c + vec2(-1.5, 3.5), c + vec2(4.5, -3.0)],
            Stroke::new(2.0_f32, Color32::WHITE),
        );
    } else {
        p.rect_stroke(
            b,
            CornerRadius::same(5),
            Stroke::new(1.3_f32, MUTED),
            StrokeKind::Inside,
        );
    }
    p.text(
        pos2(b.max.x + 10.0, r.center().y),
        Align2::LEFT_CENTER,
        label,
        FontId::proportional(13.5),
        if *on { TEXT } else { MUTED },
    );
}

fn button(ui: &mut egui::Ui, r: Rect, text: &str, primary: bool) -> egui::Response {
    let resp = ui.interact(r, ui.id().with(text), Sense::click());
    let p = ui.painter();
    let (fill, fg) = if primary {
        (
            if resp.hovered() {
                Color32::from_rgb(0xff, 0x66, 0x75)
            } else {
                RED
            },
            Color32::WHITE,
        )
    } else {
        (
            if resp.hovered() {
                Color32::from_rgb(0x2e, 0x30, 0x3b)
            } else {
                TRACK
            },
            TEXT,
        )
    };
    p.rect_filled(r, CornerRadius::same(8), fill);
    p.text(
        r.center(),
        Align2::CENTER_CENTER,
        text,
        FontId::proportional(14.0),
        fg,
    );
    resp
}

/// Brillo difuso: un degradado radial suave (malla con el centro opaco y el
/// borde transparente).
fn glow(p: &egui::Painter, c: Pos2, radius: f32, color: Color32, strength: f32) {
    let col = |a: f32| {
        let a = (a.clamp(0.0, 1.0) * 255.0) as u8;
        Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), a)
    };
    // (radio relativo, opacidad) desde el centro hacia afuera.
    let rings = [
        (0.0, 1.0),
        (0.35, 0.75),
        (0.6, 0.35),
        (0.8, 0.12),
        (1.0, 0.0),
    ];
    let n = 64;
    let mut mesh = egui::Mesh::default();
    for (ri, (r, a)) in rings.iter().enumerate() {
        for k in 0..n {
            let ang = k as f32 / n as f32 * std::f32::consts::TAU;
            let pos = c + vec2(ang.cos(), ang.sin()) * radius * *r;
            mesh.colored_vertex(pos, col(a * strength));
            if ri > 0 {
                let (i0, i1) = (
                    ((ri - 1) * n + k) as u32,
                    ((ri - 1) * n + (k + 1) % n) as u32,
                );
                let (o0, o1) = ((ri * n + k) as u32, (ri * n + (k + 1) % n) as u32);
                mesh.add_triangle(i0, i1, o0);
                mesh.add_triangle(i1, o1, o0);
            }
        }
    }
    p.add(mesh);
}

fn ease_out(x: f32) -> f32 {
    1.0 - (1.0 - x).powi(3)
}

fn install_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    for path in [
        r"C:\Windows\Fonts\segoeui.ttf",
        "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
    ] {
        if let Ok(bytes) = std::fs::read(path) {
            fonts.font_data.insert(
                "ui".into(),
                std::sync::Arc::new(egui::FontData::from_owned(bytes)),
            );
            fonts
                .families
                .entry(egui::FontFamily::Proportional)
                .or_default()
                .insert(0, "ui".into());
            break;
        }
    }
    ctx.set_fonts(fonts);
}

/// Esquinas redondeadas, sombra y modo oscuro en Windows 11 (igual que Neon).
fn round_window(cc: &eframe::CreationContext<'_>) {
    #[cfg(windows)]
    {
        use core::ffi::c_void;
        use raw_window_handle::{HasWindowHandle, RawWindowHandle};
        #[repr(C)]
        struct Margins {
            l: i32,
            r: i32,
            t: i32,
            b: i32,
        }
        #[link(name = "dwmapi")]
        unsafe extern "system" {
            fn DwmSetWindowAttribute(
                hwnd: isize,
                attr: u32,
                value: *const c_void,
                size: u32,
            ) -> i32;
            fn DwmExtendFrameIntoClientArea(hwnd: isize, margins: *const Margins) -> i32;
        }
        if let Ok(handle) = cc.window_handle() {
            if let RawWindowHandle::Win32(h) = handle.as_raw() {
                let hwnd = h.hwnd.get();
                let set = |attr: u32, v: u32| unsafe {
                    DwmSetWindowAttribute(hwnd, attr, &v as *const u32 as *const c_void, 4);
                };
                set(20, 1); // modo oscuro
                set(33, 2); // esquinas redondeadas
                set(34, 0x002f_2624); // borde (COLORREF 0x00BBGGRR) = #24262f
                let m = Margins {
                    l: 1,
                    r: 1,
                    t: 1,
                    b: 1,
                };
                unsafe { DwmExtendFrameIntoClientArea(hwnd, &m) };
            }
        }
    }
    #[cfg(not(windows))]
    let _ = cc;
}
