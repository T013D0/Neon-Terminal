// En release no abre una consola extra en Windows.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod commands;
mod git;
mod history;
mod install;
mod intro;
mod pane;
mod profiles;
mod session;
mod settings;
mod settings_ui;
mod shell;
mod theme;
mod themes;
mod ui;
mod update;
mod vsel;
mod quick;
mod dirs;
mod hotkey;
mod ai;
mod ports;
mod changes;
mod finder;
mod ssh;
mod safepaste;
mod project;
mod crash;
mod help;
mod window;
mod workspace;

fn main() -> eframe::Result {
    crash::install();
    // Para probar el reporte de errores: NEON_PROBAR_ERROR=1 neon
    if std::env::var_os("NEON_PROBAR_ERROR").is_some() {
        panic!("prueba del reporte de errores (NEON_PROBAR_ERROR)");
    }
    shell::migrate_old_data();
    let args: Vec<String> = std::env::args().skip(1).collect();
    // `neon --instalar` / `--desinstalar`: integra con Windows y termina.
    if let Some(code) = install::handle_cli(&args) {
        std::process::exit(code);
    }

    // Abierto desde el Explorador (`neon` en la barra, "Abrir en Neon"): solo esa carpeta.
    // Abierto normalmente: se restaura la sesión anterior.
    let start_dir = install::start_dir(args.first().map(String::as_str));
    let session = if start_dir.is_none() {
        session::load()
    } else {
        None
    };

    let mut viewport = egui::ViewportBuilder::default()
        .with_title("Neon")
        .with_icon(std::sync::Arc::new(
            eframe::icon_data::from_png_bytes(include_bytes!("../assets/icon-256.png"))
                .unwrap_or_default(),
        ))
        .with_inner_size([1200.0, 760.0])
        .with_min_inner_size([520.0, 320.0])
        // Sin barra de título de Windows: la barra de pestañas la reemplaza.
        .with_decorations(false)
        .with_resizable(true)
        // Transparente para la opacidad y Mica/Acrylic (con opacidad 100 % se ve igual).
        .with_transparent(true);
    if let Some(w) = session.as_ref().and_then(|s| s.windows.first()) {
        if let Some([x, y]) = w.pos {
            viewport = viewport.with_position([x, y]);
        }
        if let Some([wd, h]) = w.size {
            viewport = viewport.with_inner_size([wd, h]);
        }
        if w.maximized {
            viewport = viewport.with_maximized(true);
        }
    }

    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };
    eframe::run_native(
        "neon",
        options,
        Box::new(move |cc| Ok(Box::new(app::App::new(cc, session, start_dir)))),
    )
}
