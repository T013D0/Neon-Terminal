//! Todas las acciones de Neon. Las disparan los atajos, la paleta de comandos,
//! los menús y la barra de pestañas; la app las ejecuta en un solo lugar.

use crate::profiles::{Profile, ProfileKind};
use crate::workspace::Dir;

#[derive(Clone, Debug, PartialEq)]
pub enum Cmd {
    NewTab(Option<ProfileKind>),
    SplitRight,
    SplitDown,
    ClosePane,
    CloseTab(usize),
    NewWindow,
    CloseWindow,
    NextTab,
    PrevTab,
    SelectTab(usize),
    FocusDir(Dir),
    ResizeDir(Dir),
    FocusPane(u64),
    ToggleZoom,
    FontBigger,
    FontSmaller,
    FontReset,
    CopyLastOutput,
    HistorySearch,
    Palette,
    StartRename(usize),
    Rename(usize, String),
    SetColor(usize, Option<u8>),
    TabToNewWindow(usize),
    /// Junta la pestaña `from` dentro de la pestaña `into`, al lado indicado.
    JoinTab {
        from: usize,
        into: usize,
        side: Dir,
    },
    PaneToNewTab,
    Insert(u64, String),
    Run(u64, String),
    OpenSettings,
    /// Abre Ajustes en la sección Actualizaciones.
    OpenUpdates,
    CheckUpdates,
    ApplyUpdate,
    Search,
    PromptPrev,
    PromptNext,
    CopySelection(u64),
    Paste(u64),
    /// Escribe texto tal cual en la PTY (sin tocar la línea del prompt).
    Raw(u64, String),
    /// Archivos soltados o copiados: se escriben como rutas.
    PastePaths(u64, Vec<std::path::PathBuf>),
    ClearPane(u64),
    /// Cerrar sin preguntar (después de confirmar).
    ForceClosePane(u64),
    ForceCloseTab(usize),
    ForceCloseWindow,
}

/// Una acción que se puede asignar a un atajo (y cambiar en Ajustes).
pub struct Bindable {
    pub id: &'static str,
    pub label: &'static str,
    pub default: &'static str,
    pub cmd: Cmd,
}

/// Atajos configurables, con sus valores por defecto.
pub fn bindables() -> Vec<Bindable> {
    let b = |id, label, default, cmd| Bindable {
        id,
        label,
        default,
        cmd,
    };
    vec![
        b(
            "palette",
            "Paleta de comandos",
            "Ctrl+Shift+P",
            Cmd::Palette,
        ),
        b("settings", "Ajustes", "Ctrl+Comma", Cmd::OpenSettings),
        b(
            "new_tab",
            "Nueva pestaña",
            "Ctrl+Shift+T",
            Cmd::NewTab(None),
        ),
        b("close_pane", "Cerrar panel", "Ctrl+Shift+W", Cmd::ClosePane),
        b(
            "new_window",
            "Nueva ventana",
            "Ctrl+Shift+N",
            Cmd::NewWindow,
        ),
        b(
            "split_right",
            "Dividir a la derecha",
            "Ctrl+Shift+D",
            Cmd::SplitRight,
        ),
        b(
            "split_down",
            "Dividir hacia abajo",
            "Ctrl+Shift+E",
            Cmd::SplitDown,
        ),
        b("search", "Buscar en la salida", "Ctrl+Shift+F", Cmd::Search),
        b(
            "history",
            "Buscar en el historial",
            "Ctrl+R",
            Cmd::HistorySearch,
        ),
        b(
            "copy_output",
            "Copiar salida del último comando",
            "Ctrl+Shift+O",
            Cmd::CopyLastOutput,
        ),
        b(
            "zoom_pane",
            "Maximizar / restaurar panel",
            "Ctrl+Shift+Enter",
            Cmd::ToggleZoom,
        ),
        b(
            "prev_tab",
            "Pestaña anterior",
            "Ctrl+Shift+Tab",
            Cmd::PrevTab,
        ),
        b("next_tab", "Pestaña siguiente", "Ctrl+Tab", Cmd::NextTab),
        b(
            "prompt_prev",
            "Ir al comando anterior",
            "Ctrl+ArrowUp",
            Cmd::PromptPrev,
        ),
        b(
            "prompt_next",
            "Ir al comando siguiente",
            "Ctrl+ArrowDown",
            Cmd::PromptNext,
        ),
        b(
            "resize_left",
            "Achicar panel ←",
            "Alt+Shift+ArrowLeft",
            Cmd::ResizeDir(Dir::Left),
        ),
        b(
            "resize_right",
            "Agrandar panel →",
            "Alt+Shift+ArrowRight",
            Cmd::ResizeDir(Dir::Right),
        ),
        b(
            "resize_up",
            "Achicar panel ↑",
            "Alt+Shift+ArrowUp",
            Cmd::ResizeDir(Dir::Up),
        ),
        b(
            "resize_down",
            "Agrandar panel ↓",
            "Alt+Shift+ArrowDown",
            Cmd::ResizeDir(Dir::Down),
        ),
        b(
            "focus_left",
            "Ir al panel de la izquierda",
            "Alt+ArrowLeft",
            Cmd::FocusDir(Dir::Left),
        ),
        b(
            "focus_right",
            "Ir al panel de la derecha",
            "Alt+ArrowRight",
            Cmd::FocusDir(Dir::Right),
        ),
        b(
            "focus_up",
            "Ir al panel de arriba",
            "Alt+ArrowUp",
            Cmd::FocusDir(Dir::Up),
        ),
        b(
            "focus_down",
            "Ir al panel de abajo",
            "Alt+ArrowDown",
            Cmd::FocusDir(Dir::Down),
        ),
        b(
            "font_bigger",
            "Agrandar letra",
            "Ctrl+Equals",
            Cmd::FontBigger,
        ),
        b(
            "font_smaller",
            "Achicar letra",
            "Ctrl+Minus",
            Cmd::FontSmaller,
        ),
        b(
            "font_reset",
            "Tamaño de letra normal",
            "Ctrl+0",
            Cmd::FontReset,
        ),
    ]
}

/// Atajo efectivo de una acción: el del usuario o el por defecto.
/// `None` si el usuario lo dejó vacío.
pub fn binding_for(b: &Bindable, s: &crate::settings::Settings) -> Option<egui::KeyboardShortcut> {
    let text = s.keys.get(b.id).map(String::as_str).unwrap_or(b.default);
    if text.trim().is_empty() {
        None
    } else {
        crate::settings::parse_shortcut(text)
    }
}

/// Texto del atajo para mostrar (paleta, ajustes).
pub fn shortcut_label(id: &str, s: &crate::settings::Settings) -> String {
    bindables()
        .iter()
        .find(|b| b.id == id)
        .and_then(|b| binding_for(b, s))
        .map(|sc| crate::settings::format_shortcut(&sc))
        .unwrap_or_default()
}

pub struct PaletteItem {
    pub cmd: Cmd,
    pub label: String,
    pub keys: String,
}

/// Lo que aparece en la paleta (Ctrl+Shift+P), en este orden.
pub fn palette_items(
    profiles: &[Profile],
    active_tab: usize,
    tab_titles: &[String],
) -> Vec<PaletteItem> {
    let settings = crate::settings::get();
    // Si la acción tiene atajo configurable, se muestra el actual (puede haberse cambiado).
    let it = |cmd: Cmd, label: &str, keys: &str| {
        let keys = bindables()
            .iter()
            .find(|b| b.cmd == cmd)
            .map(|b| shortcut_label(b.id, &settings))
            .unwrap_or_else(|| keys.to_string());
        PaletteItem {
            cmd,
            label: label.to_string(),
            keys,
        }
    };
    let mut v = vec![
        it(Cmd::OpenSettings, "Ajustes", ""),
        it(Cmd::CheckUpdates, "Buscar actualizaciones", ""),
        it(Cmd::ApplyUpdate, "Instalar la actualización disponible", ""),
        it(Cmd::Search, "Buscar en la salida", ""),
        it(Cmd::NewTab(None), "Nueva pestaña", "Ctrl+Shift+T"),
        it(Cmd::SplitRight, "Dividir a la derecha", "Ctrl+Shift+D"),
        it(Cmd::SplitDown, "Dividir hacia abajo", "Ctrl+Shift+E"),
        it(Cmd::ClosePane, "Cerrar panel", "Ctrl+Shift+W"),
        it(Cmd::CloseTab(active_tab), "Cerrar pestaña", ""),
        it(Cmd::NewWindow, "Nueva ventana", "Ctrl+Shift+N"),
        it(
            Cmd::CopyLastOutput,
            "Copiar salida del último comando",
            "Ctrl+Shift+O",
        ),
        it(Cmd::HistorySearch, "Buscar en el historial", "Ctrl+R"),
        it(
            Cmd::ToggleZoom,
            "Maximizar / restaurar panel",
            "Ctrl+Shift+Enter",
        ),
        it(
            Cmd::StartRename(active_tab),
            "Renombrar pestaña",
            "doble click",
        ),
        it(
            Cmd::TabToNewWindow(active_tab),
            "Mover pestaña a ventana nueva",
            "",
        ),
        it(Cmd::PaneToNewTab, "Mover panel a pestaña nueva", ""),
        it(Cmd::PromptPrev, "Ir al comando anterior", ""),
        it(Cmd::PromptNext, "Ir al comando siguiente", ""),
        it(Cmd::NextTab, "Pestaña siguiente", "Ctrl+Tab"),
        it(Cmd::PrevTab, "Pestaña anterior", "Ctrl+Shift+Tab"),
        it(
            Cmd::FocusDir(Dir::Left),
            "Ir al panel de la izquierda",
            "Alt+←",
        ),
        it(
            Cmd::FocusDir(Dir::Right),
            "Ir al panel de la derecha",
            "Alt+→",
        ),
        it(Cmd::FocusDir(Dir::Up), "Ir al panel de arriba", "Alt+↑"),
        it(Cmd::FocusDir(Dir::Down), "Ir al panel de abajo", "Alt+↓"),
        it(
            Cmd::ResizeDir(Dir::Right),
            "Agrandar panel →",
            "Alt+Shift+→",
        ),
        it(Cmd::ResizeDir(Dir::Left), "Achicar panel ←", "Alt+Shift+←"),
        it(Cmd::FontBigger, "Agrandar letra", "Ctrl+="),
        it(Cmd::FontSmaller, "Achicar letra", "Ctrl+-"),
        it(Cmd::FontReset, "Tamaño de letra normal", "Ctrl+0"),
        it(
            Cmd::SetColor(active_tab, None),
            "Quitar color de la pestaña",
            "",
        ),
    ];
    for p in profiles {
        v.push(it(
            Cmd::NewTab(Some(p.kind)),
            &format!("Nueva pestaña: {}", p.name),
            "",
        ));
    }
    for (i, name) in ["rojo", "naranjo", "amarillo", "verde", "celeste", "violeta"]
        .iter()
        .enumerate()
    {
        v.push(it(
            Cmd::SetColor(active_tab, Some(i as u8)),
            &format!("Color de pestaña: {name}"),
            "",
        ));
    }
    // Juntar otra pestaña ya abierta con la actual, en pantalla dividida.
    for (i, title) in tab_titles.iter().enumerate() {
        if i == active_tab {
            continue;
        }
        for (side, where_) in [(Dir::Right, "a la derecha"), (Dir::Down, "abajo")] {
            v.push(it(
                Cmd::JoinTab {
                    from: i,
                    into: active_tab,
                    side,
                },
                &format!("Dividir con pestaña {} «{title}» {where_}", i + 1),
                "",
            ));
        }
    }
    v
}
